use owo_colors::OwoColorize;

#[derive(Clone, Copy, Debug)]
pub enum Tone {
    Brand,
    Heading,
    Label,
    Value,
    Muted,
    Success,
    Warning,
    Error,
}

pub struct Ui;

impl Ui {
    pub const DIVIDER: &'static str = "────────────────────────────────────";

    pub fn text(tone: Tone, value: impl AsRef<str>) -> String {
        let value = value.as_ref();
        match tone {
            Tone::Brand => value.bold().cyan().to_string(),
            Tone::Heading => value.bold().bright_white().to_string(),
            Tone::Label => value.bold().blue().to_string(),
            Tone::Value => value.bold().white().to_string(),
            Tone::Muted => value.dimmed().to_string(),
            Tone::Success => value.bold().green().to_string(),
            Tone::Warning => value.bold().yellow().to_string(),
            Tone::Error => value.bold().red().to_string(),
        }
    }

    pub fn divider() {
        println!("{}", Self::text(Tone::Muted, Self::DIVIDER));
    }

    pub fn title(title: &str) {
        println!(
            "{} {}",
            Self::text(Tone::Brand, "^.^"),
            Self::text(Tone::Heading, title)
        );
    }

    pub fn success(message: &str) {
        println!("{} {}", Self::text(Tone::Success, "✓"), message);
    }

    pub fn warning(message: &str) {
        println!("{} {}", Self::text(Tone::Warning, "!"), message);
    }

    pub fn error(message: &str) {
        eprintln!("{} {}", Self::text(Tone::Error, "×"), message);
    }
}
