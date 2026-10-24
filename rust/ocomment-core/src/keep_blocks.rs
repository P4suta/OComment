#[derive(Clone, Copy, Debug)]
pub(crate) struct Member {
    pub(crate) joins_previous: bool,
    pub(crate) pattern: Option<usize>,
}

pub(crate) fn adjacent(gap: &[u8]) -> bool {
    let mut lines = 0u8;
    let mut after_cr = false;
    for &byte in gap {
        match byte {
            b'\r' => {
                lines += 1;
                after_cr = true;
            }
            b'\n' => {
                if !after_cr {
                    lines += 1;
                }
                after_cr = false;
            }
            _ if byte.is_ascii_whitespace() => after_cr = false,
            _ => return false,
        }
        if lines > 1 {
            return false;
        }
    }
    lines == 1
}

pub(crate) fn kept_patterns(members: &[Member]) -> Vec<Option<usize>> {
    let mut kept = Vec::with_capacity(members.len());
    let mut pattern = None;
    for member in members {
        if !member.joins_previous {
            pattern = None;
        }
        if let Some(candidate) = member.pattern {
            pattern = Some(pattern.map_or(candidate, |previous: usize| previous.min(candidate)));
        }
        kept.push(pattern);
    }
    for index in (1..members.len()).rev() {
        if members[index].joins_previous {
            kept[index - 1] = kept[index];
        }
    }
    kept
}
