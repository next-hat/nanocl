use std::io::{Write, stdout};

use nanocl_error::io::{IoError, IoResult};
use termimad::crossterm::{
  cursor::{Hide, Show},
  event::{self, Event, KeyCode::*, KeyEvent},
  queue,
  style::{Attribute, Color::*},
  terminal::{
    self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
  },
};
use termimad::*;

fn view_area() -> Area {
  // area.pad_for_max_width(120);
  Area::full_screen()
}

fn make_skin() -> MadSkin {
  let mut skin = MadSkin::default();
  skin.set_bg(Black);
  // Make the layout take 100% of the width without margins or anything
  skin.paragraph.set_fg(Grey);
  skin.paragraph.left_margin = 2;
  skin.table.align = Alignment::Left;
  skin.table.left_margin = 2;
  for h in &mut skin.headers {
    h.set_fg(Red);
    h.add_attr(Attribute::NoUnderline);
  }
  skin.headers[0].set_fg(White);
  skin.bold.set_fg(Green);
  skin.italic.set_fg(Magenta);
  // Hide scrollbar by making it black on black
  skin.scrollbar.set_bg(Black);
  skin.scrollbar.track.set_fg(Black);
  skin.scrollbar.track.set_bg(Black);
  skin.scrollbar.thumb.set_fg(Black);
  skin.scrollbar.thumb.set_bg(Black);
  skin.code_block.align = Alignment::Left;
  skin.code_block.set_bg(AnsiValue(0));
  // skin.code_block.set_fg(White);
  skin.code_block.add_attr(Attribute::Dim);
  skin.code_block.left_margin = 2;
  skin
}

/// Display markdown in a scrollable full-screen terminal view.
/// Controls: Up/Down arrows, PageUp/PageDown. Any other key to quit.
pub fn display(md: &str) -> IoResult<()> {
  let skin = make_skin();
  let mut w = stdout();
  queue!(w, EnterAlternateScreen)?;
  terminal::enable_raw_mode()?;
  queue!(w, Hide)?;
  let mut view = MadView::from(md.to_owned(), view_area(), skin);
  loop {
    view.write_on(&mut w).map_err(|e| {
      IoError::other("Failed to write to stdout", e.to_string().as_str())
    })?;
    w.flush()?;
    match event::read() {
      Ok(Event::Key(KeyEvent { code, .. })) => match code {
        Up => view.try_scroll_lines(-1),
        Down => view.try_scroll_lines(1),
        PageUp => view.try_scroll_pages(-1),
        PageDown => view.try_scroll_pages(1),
        _ => break,
      },
      Ok(Event::Resize(..)) => {
        queue!(w, Clear(ClearType::All))?;
        view.resize(&view_area());
      }
      _ => {}
    }
  }
  terminal::disable_raw_mode()?;
  queue!(w, Show)?;
  queue!(w, LeaveAlternateScreen)?;
  w.flush()?;
  Ok(())
}
