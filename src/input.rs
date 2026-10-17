use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Text(char),
    Backspace,
    Confirm,
    Cancel,
    NextField,
    PreviousField,
    Left,
    Right,
    ToggleMode,
    CycleDuration,
    Quit,
    PasteRejected,
    SuspendRejected,
    Resize(u16, u16),
}

pub fn normalize(event: Event) -> Option<Action> {
    match event {
        Event::Resize(width, height) => Some(Action::Resize(width, height)),
        Event::Paste(_) => Some(Action::PasteRejected),
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                return match key.code {
                    KeyCode::Char('c' | 'C') => Some(Action::Quit),
                    KeyCode::Char('z' | 'Z') => Some(Action::SuspendRejected),
                    _ => None,
                };
            }
            if key.modifiers.intersects(
                KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::HYPER | KeyModifiers::META,
            ) {
                return None;
            }
            match key.code {
                KeyCode::Char(c) => Some(Action::Text(c)),
                KeyCode::Backspace => Some(Action::Backspace),
                _ if key.kind == KeyEventKind::Repeat => None,
                KeyCode::Enter => Some(Action::Confirm),
                KeyCode::Esc => Some(Action::Cancel),
                KeyCode::Tab | KeyCode::Down => Some(Action::NextField),
                KeyCode::BackTab | KeyCode::Up => Some(Action::PreviousField),
                KeyCode::Left => Some(Action::Left),
                KeyCode::Right => Some(Action::Right),
                KeyCode::F(2) => Some(Action::ToggleMode),
                KeyCode::F(3) => Some(Action::CycleDuration),
                _ => None,
            }
        }
        _ => None,
    }
}
