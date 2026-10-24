#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConstraintStart {
    Equals,
    CloseAngle,
    Slash,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AfterName {
    Comma,
    Equals,
    Extends(ConstraintStart),
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Goal {
    TypeParameters,
    Jsx,
}

pub(crate) const fn goal(after: AfterName) -> Goal {
    match after {
        AfterName::Comma | AfterName::Equals | AfterName::Extends(ConstraintStart::Other) => {
            Goal::TypeParameters
        }
        AfterName::Extends(
            ConstraintStart::Equals | ConstraintStart::CloseAngle | ConstraintStart::Slash,
        )
        | AfterName::Other => Goal::Jsx,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct UnicodeEscape {
    width: usize,
    scalar: char,
}

impl UnicodeEscape {
    pub(crate) const fn width(self) -> usize {
        self.width
    }

    pub(crate) const fn scalar(self) -> char {
        self.scalar
    }
}

pub(crate) fn unicode_escape(bytes: &[u8]) -> Option<UnicodeEscape> {
    const PREFIX: &[u8] = b"\\u";
    const FIXED_HEX_DIGITS: usize = 4;
    const HEX_RADIX: u32 = 16;
    let rest = bytes.strip_prefix(PREFIX)?;
    let braced = rest.first() == Some(&b'{');
    let mut cursor = PREFIX.len() + usize::from(braced);
    let mut digits = 0;
    let mut value = 0u32;
    while braced || digits < FIXED_HEX_DIGITS {
        let Some(&byte) = bytes.get(cursor) else {
            break;
        };
        let Some(digit) = char::from(byte).to_digit(HEX_RADIX) else {
            break;
        };
        value = value * HEX_RADIX + digit;
        if value > char::MAX as u32 {
            return None;
        }
        digits += 1;
        cursor += 1;
    }
    if braced {
        if digits == 0 || bytes.get(cursor) != Some(&b'}') {
            return None;
        }
        cursor += 1;
    } else if digits != FIXED_HEX_DIGITS {
        return None;
    }
    Some(UnicodeEscape {
        width: cursor,
        scalar: char::from_u32(value)?,
    })
}
