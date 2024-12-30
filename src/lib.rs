use std::fmt::{Display, Formatter};

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

    pub fn pixel_command_at(&self, x: u16, y: u16) -> String {
        format!("PX {x} {y} {self}\n")
    }
}

impl Display for Color {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.alpha == 0xFF && self.red == self.green && self.green == self.blue {
            write!(f, "{:02x}", self.red)
        }else if self.alpha == 0xFF {
            write!(f, "{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
        }else {
            write!(f, "{:02x}{:02x}{:02x}{:02x}", self.alpha, self.red, self.green, self.blue)
        }
    }
}

pub struct PixelBatch {
    offset: Pos,
    pixels: Vec<(Pos, Color)>,
}

impl PixelBatch {
    pub fn new(offset: Pos, capacity: usize) -> Self {
        Self {offset, pixels: Vec::with_capacity(capacity) }
    }

    pub fn add(&mut self, abs_pos: Pos, color: Color) {
        if abs_pos.x < self.offset.x || abs_pos.y < self.offset.y {
            panic!("Tried to add pixel at {abs_pos:?} while offset was {:?}", self.offset)
        }

        self.pixels.push((abs_pos, color));
    }

    pub fn optimize(&mut self) {
        if self.len() == 0 {
            return;
        }

        let mut min_offset = Pos::new(u16::MAX, u16::MAX);
        for pixel in self.pixels.iter() {
            if pixel.0.x < min_offset.x {
                min_offset.x = pixel.0.x;
            }
            if pixel.0.y < min_offset.y {
                min_offset.y = pixel.0.y;
            }
        }

        if min_offset != self.offset {
            self.offset = min_offset;
        }
    }

    pub fn clear(&mut self) {
        self.pixels.clear();
    }

    pub fn offset(&self) -> Pos {
        self.offset
    }

    pub fn pixels(&self) -> &Vec<(Pos, Color)> {
        &self.pixels
    }

    pub fn len(&self) -> usize {
        self.pixels.len()
    }

    pub fn commands(&self) -> String {
        match self.len() {
            0 => String::new(),
            1 => self.pixels[0].1.pixel_command_at(self.offset.x + self.pixels[0].0.x, self.offset.y + self.pixels[0].0.y),
            _ => {
                let mut commands = String::with_capacity("PX 1000 1000 RRGGBB\n".len() * self.len() + "OFFSET 1000 1000\n".len());
                commands.push_str(&format!("OFFSET {} {}\n", self.offset.x, self.offset.y));
                for pixel in self.pixels.iter() {
                    commands.push_str(&pixel.1.pixel_command_at(pixel.0.x - self.offset.x, pixel.0.y - self.offset.y));
                }
                commands
            }
        }
    }
}