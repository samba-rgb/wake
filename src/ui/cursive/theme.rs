use cursive::theme::{Theme, Color, PaletteColor, BaseColor};

pub fn create_wake_theme() -> Theme {
    let mut theme = Theme::default();
    
    // Customize colors for Wake's log viewer
    theme.palette[PaletteColor::Background] = Color::TerminalDefault;
    theme.palette[PaletteColor::View] = Color::TerminalDefault;
    theme.palette[PaletteColor::Primary] = Color::Dark(BaseColor::White);
    theme.palette[PaletteColor::Secondary] = Color::Dark(BaseColor::Blue);
    theme.palette[PaletteColor::Tertiary] = Color::Dark(BaseColor::Cyan);
    theme.palette[PaletteColor::TitlePrimary] = Color::Dark(BaseColor::Yellow);
    theme.palette[PaletteColor::Highlight] = Color::Dark(BaseColor::Green);
    theme.palette[PaletteColor::HighlightInactive] = Color::Dark(BaseColor::Blue);
    
    theme
}