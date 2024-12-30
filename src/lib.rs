#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub x: u16,
    pub y: u16,
}

impl Pos {
    pub fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue, alpha: 0xFF }
    }

    pub fn new_alpha(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self { red, green, blue, alpha }
    }
}

pub fn to_hex(r: u8, g: u8, b: u8) -> String {
    if r == g && g == b {
        format!("{:02x}", r)
    }else {
        format!("{:02x}{:02x}{:02x}", r, g, b)
    }
}
