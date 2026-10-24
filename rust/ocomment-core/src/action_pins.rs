const SHA1_WIDTH: usize = 40;
const SHA256_WIDTH: usize = 64;
const LOCK_SUFFIX: &[u8] = b".lock";

pub(crate) fn is_pin(bytes: &[u8]) -> bool {
    matches!(bytes.len(), SHA1_WIDTH | SHA256_WIDTH) && bytes.iter().all(u8::is_ascii_hexdigit)
}

pub(crate) fn is_reference(bytes: &[u8]) -> bool {
    if bytes.is_empty()
        || bytes == b"@"
        || matches!(bytes.last(), Some(b'.' | b'/'))
        || bytes.ends_with(LOCK_SUFFIX)
    {
        return false;
    }
    let mut previous = b'/';
    for (index, &byte) in bytes.iter().enumerate() {
        if byte <= b' '
            || matches!(
                byte,
                b'\x7f' | b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\'
            )
            || byte == b'.' && matches!(previous, b'.' | b'/')
            || byte == b'/' && (previous == b'/' || bytes[..index].ends_with(LOCK_SUFFIX))
            || byte == b'{' && previous == b'@'
        {
            return false;
        }
        previous = byte;
    }
    true
}

fn numeric_start(bytes: &[u8]) -> bool {
    let value = bytes.strip_prefix(b"v").unwrap_or(bytes);
    value.first().is_some_and(u8::is_ascii_digit)
}

fn is_version_token(bytes: &[u8]) -> bool {
    if !is_reference(bytes) {
        return false;
    }
    if numeric_start(bytes) {
        return true;
    }
    let mut prefix = true;
    for (index, &byte) in bytes.iter().enumerate() {
        if prefix && matches!(byte, b'-' | b'/') && numeric_start(&bytes[index + 1..]) {
            return true;
        }
        prefix &= byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-');
    }
    false
}

pub(crate) fn is_annotation(bytes: &[u8]) -> bool {
    let body = bytes.trim_ascii();
    if is_reference(body) {
        return true;
    }
    let version = if let Some(rest) = body.strip_prefix(b"pin")
        && rest.first().is_some_and(u8::is_ascii_whitespace)
    {
        rest.trim_ascii_start()
    } else if let Some(rest) = body.strip_prefix(b"tag")
        && let Some(value) = rest.trim_ascii_start().strip_prefix(b"=")
    {
        value.trim_ascii_start()
    } else {
        body
    };
    let end = version
        .iter()
        .position(u8::is_ascii_whitespace)
        .unwrap_or(version.len());
    is_version_token(&version[..end])
}

struct PinnedAction;

fn uses_pin(bytes: &[u8]) -> Option<PinnedAction> {
    let bytes = bytes.trim_ascii();
    let bytes = if let Some(rest) = bytes.strip_prefix(b"-") {
        if !rest.first().is_some_and(u8::is_ascii_whitespace) {
            return None;
        }
        rest.trim_ascii_start()
    } else {
        bytes
    };
    let colon = bytes.iter().position(|byte| *byte == b':')?;
    if !matches!(
        bytes[..colon].trim_ascii(),
        b"uses" | b"'uses'" | b"\"uses\""
    ) {
        return None;
    }
    let value = &bytes[colon + 1..];
    if !value
        .first()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        return None;
    }
    let value = value.trim_ascii();
    let value = match value.first()? {
        quote @ (b'\'' | b'"') => {
            if value.len() < 2 || value.last() != Some(quote) {
                return None;
            }
            &value[1..value.len() - 1]
        }
        _ => value,
    };
    let at = value.iter().rposition(|byte| *byte == b'@')?;
    if !is_pin(&value[at + 1..]) {
        return None;
    }
    let action = &value[..at];
    if !action
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/'))
    {
        return None;
    }
    let mut parts = action.split(|byte| *byte == b'/');
    let owner = parts.next()?;
    let repository = parts.next()?;
    if owner.is_empty()
        || repository.is_empty()
        || matches!(owner, b"." | b"..")
        || matches!(repository, b"." | b"..")
        || parts.any(|part| part.is_empty() || matches!(part, b"." | b".."))
    {
        return None;
    }
    Some(PinnedAction)
}

pub(crate) fn annotation(prefix: &[u8], raw: &[u8]) -> bool {
    let Some(PinnedAction) = uses_pin(prefix) else {
        return false;
    };
    let Some(body) = raw.strip_prefix(b"#") else {
        return false;
    };
    is_annotation(body)
}
