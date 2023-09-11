use owo_colors::AnsiColors;

fn pick_colour(num: u64) -> AnsiColors {
    match num {
        1 => AnsiColors::Red,
        2 => AnsiColors::Green,
        3 => AnsiColors::Yellow,
        4 => AnsiColors::Blue,
        5 => AnsiColors::Magenta,
        6 => AnsiColors::Cyan,
        7 => AnsiColors::BrightRed,
        8 => AnsiColors::BrightGreen,
        9 => AnsiColors::BrightYellow,
        10 => AnsiColors::BrightBlue,
        11 => AnsiColors::BrightMagenta,
        12 => AnsiColors::BrightCyan,
        _ => AnsiColors::White,
    }
}

/// Pick a colour based on the sum of the base16 digits comprising
/// the given public key.
pub fn public_key_to_colour(public_key: &[u8; 32]) -> AnsiColors {
    // A return type of `u64` is used to avoid the overflow which will
    // likely occur if returning `u8`.
    let sum: u64 = public_key.iter().map(|x| *x as u64).sum();

    pick_colour(sum % 12)
}
