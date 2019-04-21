use tuikit::cell::Cell;
use tuikit::prelude::*;

use std::fs;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::thread;

struct Nash {
    term: Term,
    row: usize,
    col: usize,
    buffer: String,
    sugg_cache: Vec<String>,
    rec: Receiver<String>,
    sug_idx: usize,
    cur_sugs: Vec<String>,
}

impl Nash {
    fn new() -> Result<Self> {
        let rec = Self::fill_cache();
        Ok(Self {
            term: Term::with_height(TermHeight::Percent(100))?,
            row: 0,
            col: 9,
            buffer: String::new(),
            sugg_cache: Vec::new(),
            rec,
            sug_idx: 0,
            cur_sugs: Vec::new(),
        })
    }
    fn start(&mut self) -> Result<()> {
        self.term.clear()?;
        self.print_msg_at(0, 0, "nash~>> ", Color::YELLOW)?;

        while let Ok(ev) = self.term.poll_event() {
            if let Ok(sug) = self.rec.recv() {
                self.sugg_cache.push(sug);
            }

            match ev {
                Event::Key(Key::ESC) | Event::Key(Key::Char('q')) => break,
                Event::Key(Key::Char(c)) => self.handle_key(c)?,

                // run cmd if that fails, show uknown cmd output
                Event::Key(Key::Enter) => {
                    if self.run_cmd().is_err() {
                        self.uknown_cmd()?
                    }
                }

                Event::Key(Key::Tab) => self.cycle_sug()?,
                _ => {}
            }
        }

        Ok(())
    }

    // Ui stuff
    fn need_scrolldown(&mut self) -> Result<bool> {
        let (_, height) = self.term.term_size()?;
        if self.row as f32 >= height as f32 * 0.98 {
            return Ok(true);
        }
        Ok(false)
    }
    fn raw_print(&mut self, mut row: usize, col: usize, msg: &str, color: Color) -> Result<()> {
        if self.need_scrolldown()? {
            self.term.clear()?;
            row = 0;
            self.row = 1;
        };
        self.term.print_with_attr(
            row,
            col,
            msg,
            Attr {
                fg: color,
                ..Default::default()
            },
        )?;
        self.term.present()?;
        Ok(())
    }
    fn print_msg_at(&mut self, row: usize, col: usize, msg: &str, color: Color) -> Result<()> {
        self.clear_line()?;
        self.raw_print(row, col, msg, color)?;
        Ok(())
    }
    fn _print_msg(&mut self, msg: &str, color: Color) -> Result<()> {
        self.print_msg_at(self.row, self.col, msg, color)?;
        Ok(())
    }

    fn print_char_at(&mut self, row: usize, col: usize, c: char, color: Color) -> Result<()> {
        self.clear_line()?;
        let cell = Cell {
            attr: Attr {
                fg: color,
                ..Default::default()
            },
            ch: c,
        };
        self.term.put_cell(row, col, cell)?;

        self.term.present()?;
        Ok(())
    }

    fn print_char(&mut self, c: char, color: Color) -> Result<()> {
        self.print_char_at(self.row, self.col, c, color)?;
        Ok(())
    }

    fn clear_line(&mut self) -> Result<()> {
        self.raw_print(
            self.row,
            self.col,
            &std::iter::repeat(" ").take(100).collect::<String>(),
            Color::BLUE,
        )?;

        self.term.print_with_attr(
            self.row,
            0,
            "nash~>> ",
            Attr {
                fg: Color::YELLOW,
                ..Default::default()
            },
        )?;

        Ok(())
    }

    // cmd stuff

    fn run_cmd(&mut self) -> Result<()> {
        self.clear_line()?;

        let mut cmd_iter = self.buffer.trim().split_whitespace();
        let mut row_n = 1;
        let cmd = cmd_iter.next();

        let output = if let Some(cmd) = cmd {
            let out = &Command::new(cmd)
                .args(&cmd_iter.collect::<Vec<&str>>())
                .output()?;
            let out = if !out.stdout.is_empty() {
                &out.stdout
            } else {
                &out.stderr
            };
            String::from_utf8_lossy(out).to_string()
        } else {
            "".to_string()
        };

        for c in output.split('\n') {
            self.raw_print(self.row + row_n, 0, &c, Color::MAGENTA)?;
            row_n += 1;
        }

        self.buffer.clear();
        self.col = 9;
        if row_n != 0 {
            self.row += row_n - 1;
        }
        self.clear_line()?;
        self.term.present()?;
        Ok(())
    }

    // suggestions
    fn fill_cache() -> Receiver<String> {
        let (sd, rec) = channel();

        thread::spawn(move || {
            let paths = fs::read_dir("/usr/bin").unwrap();

            paths.for_each(|path| {
                sd.send(
                    path.unwrap()
                        .path()
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                )
                .unwrap();
            });
        });

        rec
    }

    fn find_suggestions(&mut self) {
        let search = match self.buffer.split_whitespace().last() {
            Some(s) => s,
            None => return,
        };
        self.cur_sugs = self
            .sugg_cache
            .iter()
            .filter(|s| s.starts_with(search))
            .cloned()
            .collect();
        if self.cur_sugs.is_empty() && self.get_more_sugs() {
            self.find_suggestions();
        }
    }

    fn get_more_sugs(&mut self) -> bool {
        while let Ok(sug) = self.rec.recv() {
            self.sugg_cache.push(sug.clone());
            if sug.starts_with(&self.buffer) {
                return true;
            }
        }
        false
    }

    fn print_suggesstions(&mut self, sug: &str) -> Result<()> {
        self.print_msg_at(
            self.row,
            self.col,
            &sug[self.buffer.split_whitespace().last().unwrap().len()..],
            Color::LIGHT_BLUE,
        )?;
        Ok(())
    }

    fn cycle_sug(&mut self) -> Result<()> {
        self.find_suggestions();
        self.sug_idx += 1;
        if self.sug_idx >= self.cur_sugs.len() {
            if self.get_more_sugs() {
                self.find_suggestions();
            } else {
                self.sug_idx = 0;
            }
        }
        //dbg!(&self.cur_sugs);
        if self.cur_sugs.is_empty() {
            return Ok(());
        }
        if self.cur_sugs.len() == 1 {
            for c in self.cur_sugs[0].clone()
                [self.buffer.split_whitespace().last().unwrap().len()..]
                .chars()
            {
                self.handle_key(c)?;
            }
            self.handle_key(' ')?;
        } else {
            self.print_suggesstions(&self.cur_sugs[self.sug_idx].clone())?;
        }

        Ok(())
    }

    // handle events

    fn handle_key(&mut self, c: char) -> Result<()> {
        self.buffer.push(c);
        self.print_char(c, Color::BLUE)?;
        self.col += 1;

        self.find_suggestions();
        self.sug_idx = 0;

        if self.cur_sugs.is_empty() {
            return Ok(());
        }

        self.print_suggesstions(&self.cur_sugs[0].clone())?;
        Ok(())
    }

    // default msg's
    fn uknown_cmd(&mut self) -> Result<()> {
        self.raw_print(
            self.row + 1,
            0,
            &format!("nash: Uknown command: {}", &self.buffer),
            Color::RED,
        )?;
        self.buffer.clear();
        self.col = 9;
        self.row += 2;
        self.clear_line()?;
        self.term.present()?;

        Ok(())
    }
}

fn main() {
    let mut nash = Nash::new().expect("Error while starting nash");
    nash.start().expect("Error while starting nash");
}
