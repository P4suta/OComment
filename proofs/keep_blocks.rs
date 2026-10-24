#[kani::proof]
#[kani::unwind(17)]
fn adjacency_up_to_16_bytes_matches_logical_line_endings() {
    let bytes: [u8; 16] = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= bytes.len());
    let gap = &bytes[..length];
    let mut endings = 0;
    for (index, byte) in gap.iter().enumerate() {
        if *byte == b'\r' || *byte == b'\n' && (index == 0 || gap[index - 1] != b'\r') {
            endings += 1;
        }
    }
    let expected = gap.iter().all(u8::is_ascii_whitespace) && endings == 1;
    let actual = production::adjacent(gap);
    assert_eq!(
        actual, expected,
        "adjacency agrees with the bounded byte specification"
    );
    kani::cover!(actual);
    kani::cover!(!actual);
}

fn check_run<const N: usize>() {
    let members: [production::Member; N] = std::array::from_fn(|_| production::Member {
        joins_previous: kani::any(),
        pattern: kani::any(),
    });
    let actual = production::kept_patterns(&members);
    assert_eq!(actual.len(), N, "every member receives one decision");
    if N == 0 {
        kani::cover!(actual.is_empty());
    } else {
        let index: usize = kani::any();
        kani::assume(index < N);
        let mut first = index;
        while first > 0 && members[first].joins_previous {
            first -= 1;
        }
        let mut last = index + 1;
        while last < N && members[last].joins_previous {
            last += 1;
        }
        let mut expected = None;
        for member in &members[first..last] {
            if let Some(pattern) = member.pattern {
                expected = Some(expected.map_or(pattern, |previous: usize| previous.min(pattern)));
            }
        }
        assert_eq!(
            actual[index], expected,
            "only matches inside this run keep its member"
        );
        kani::cover!(actual[index].is_some());
        kani::cover!(actual[index].is_none());
    }
}

#[kani::proof]
#[kani::unwind(9)]
fn blocks_of_8_members_keep_exactly_the_matching_run() {
    check_run::<8>();
}

#[kani::proof]
#[kani::unwind(8)]
fn blocks_of_7_members_keep_exactly_the_matching_run() {
    check_run::<7>();
}

#[kani::proof]
#[kani::unwind(7)]
fn blocks_of_6_members_keep_exactly_the_matching_run() {
    check_run::<6>();
}

#[kani::proof]
#[kani::unwind(6)]
fn blocks_of_5_members_keep_exactly_the_matching_run() {
    check_run::<5>();
}

#[kani::proof]
#[kani::unwind(5)]
fn blocks_of_4_members_keep_exactly_the_matching_run() {
    check_run::<4>();
}

#[kani::proof]
#[kani::unwind(4)]
fn blocks_of_3_members_keep_exactly_the_matching_run() {
    check_run::<3>();
}

#[kani::proof]
#[kani::unwind(3)]
fn blocks_of_2_members_keep_exactly_the_matching_run() {
    check_run::<2>();
}

#[kani::proof]
#[kani::unwind(2)]
fn blocks_of_1_members_keep_exactly_the_matching_run() {
    check_run::<1>();
}

#[kani::proof]
#[kani::unwind(1)]
fn blocks_of_0_members_keep_exactly_the_matching_run() {
    check_run::<0>();
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(17)]
fn reject_joining_across_a_blank_line() {
    assert!(
        production::adjacent(b"\n\n"),
        "probe: joining across a blank line"
    );
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(17)]
fn reject_joining_across_code() {
    assert!(
        production::adjacent(b"\ncode"),
        "probe: joining across code"
    );
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(17)]
fn reject_counting_crlf_as_two_endings() {
    assert!(!production::adjacent(b"\r\n"), "probe: counting CRLF twice");
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(17)]
fn reject_keeping_only_the_matching_member() {
    let members = [
        production::Member {
            joins_previous: false,
            pattern: None,
        },
        production::Member {
            joins_previous: true,
            pattern: Some(0),
        },
        production::Member {
            joins_previous: true,
            pattern: None,
        },
    ];
    let kept = production::kept_patterns(&members);
    assert_eq!(kept[0], None, "probe: keeping only the matching member");
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(17)]
fn reject_keeping_a_separate_run() {
    let members = [
        production::Member {
            joins_previous: false,
            pattern: Some(0),
        },
        production::Member {
            joins_previous: false,
            pattern: None,
        },
    ];
    let kept = production::kept_patterns(&members);
    assert_eq!(kept[1], Some(0), "probe: keeping a separate run");
}

#[kani::proof]
#[kani::unwind(7)]
fn proof_admission_requires_every_fact() {
    let facts: [bool; 6] = kani::any();
    let evidence = admission::Evidence {
        process: facts[0],
        completed: facts[1],
        identities: facts[2],
        version: facts[3],
        properties: facts[4],
        source: facts[5],
    };
    let expected = facts == [true; 6];
    let actual = admission::admit(evidence);
    assert_eq!(
        actual, expected,
        "proof admission requires every verified fact"
    );
    kani::cover!(actual);
    kani::cover!(!actual);
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(1)]
fn reject_incomplete_proof_evidence() {
    let evidence = admission::Evidence {
        process: true,
        completed: false,
        identities: true,
        version: true,
        properties: true,
        source: true,
    };
    assert!(
        admission::admit(evidence),
        "probe: accepting incomplete proof evidence"
    );
}

#[kani::proof]
#[kani::unwind(25)]
fn biome_keywords_up_to_24_bytes_match_only_four_headers() {
    let bytes: [u8; 24] = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= bytes.len());
    let keyword = &bytes[..length];
    let expected = if keyword.starts_with(b"biome-ignore") {
        match &keyword[12..] {
            b"" => Some(biome::Suppression::NextLine),
            b"-all" => Some(biome::Suppression::WholeFile),
            b"-start" => Some(biome::Suppression::RangeStart),
            b"-end" => Some(biome::Suppression::RangeEnd),
            _ => None,
        }
    } else {
        None
    };
    let actual = biome::recognize(keyword);
    assert_eq!(
        actual, expected,
        "only four exact Biome keyword byte sequences are accepted"
    );
    if let Some(suppression) = actual {
        assert_eq!(
            suppression.name().as_bytes(),
            keyword,
            "a recognized Biome keyword keeps its name"
        );
    }
    kani::cover!(actual == Some(biome::Suppression::NextLine));
    kani::cover!(actual == Some(biome::Suppression::WholeFile));
    kani::cover!(actual == Some(biome::Suppression::RangeStart));
    kani::cover!(actual == Some(biome::Suppression::RangeEnd));
    kani::cover!(actual.is_none());
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(25)]
fn reject_a_biome_marker_prefix_in_a_longer_word() {
    assert!(
        biome::recognize(b"biome-ignoreish").is_some(),
        "probe: protecting a Biome marker prefix"
    );
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(25)]
fn reject_an_unknown_biome_suppression_keyword() {
    assert!(
        biome::recognize(b"biome-ignore-next").is_some(),
        "probe: protecting an unknown Biome suppression keyword"
    );
}

#[kani::proof]
#[kani::unwind(1)]
fn tsx_goals_cover_the_complete_lookahead_table() {
    use tsx::{AfterName, ConstraintStart, Goal};
    let choice: u8 = kani::any();
    kani::assume(choice < 7);
    let after = match choice {
        0 => AfterName::Comma,
        1 => AfterName::Equals,
        2 => AfterName::Extends(ConstraintStart::Equals),
        3 => AfterName::Extends(ConstraintStart::CloseAngle),
        4 => AfterName::Extends(ConstraintStart::Slash),
        5 => AfterName::Extends(ConstraintStart::Other),
        _ => AfterName::Other,
    };
    let forbids_type_parameters = match after {
        AfterName::Extends(
            ConstraintStart::Equals | ConstraintStart::CloseAngle | ConstraintStart::Slash,
        )
        | AfterName::Other => true,
        AfterName::Comma | AfterName::Equals | AfterName::Extends(ConstraintStart::Other) => false,
    };
    let actual = tsx::goal(after);
    assert_eq!(
        actual == Goal::TypeParameters,
        !forbids_type_parameters,
        "TSX goal matches the complete lookahead decision table"
    );
    kani::cover!(actual == Goal::TypeParameters);
    kani::cover!(actual == Goal::Jsx);
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(1)]
fn reject_type_parameters_for_a_jsx_attribute() {
    let goal = tsx::goal(tsx::AfterName::Extends(tsx::ConstraintStart::Equals));
    assert!(
        goal == tsx::Goal::TypeParameters,
        "probe: reading a JSX attribute as a type constraint"
    );
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(1)]
fn reject_jsx_for_a_disambiguating_comma() {
    let goal = tsx::goal(tsx::AfterName::Comma);
    assert!(
        goal == tsx::Goal::Jsx,
        "probe: reading a type parameter comma as JSX"
    );
}

fn specified_unicode_escape(bytes: &[u8]) -> Option<(usize, u32)> {
    const PREFIX: &[u8] = b"\\u";
    const FIXED_HEX_DIGITS: usize = 4;
    const HEX_LETTER_OFFSET: u8 = 10;
    const BITS_PER_HEX_DIGIT: usize = 4;
    const MAXIMUM_SCALAR: u64 = 0x10ffff;
    const SURROGATES: std::ops::RangeInclusive<u64> = 0xd800..=0xdfff;
    if !bytes.starts_with(PREFIX) {
        return None;
    }
    let (digits, width) = if bytes.get(PREFIX.len()) == Some(&b'{') {
        let start = PREFIX.len() + 1;
        let close = bytes[start..].iter().position(|byte| *byte == b'}')? + start;
        (&bytes[start..close], close + 1)
    } else {
        let end = PREFIX.len() + FIXED_HEX_DIGITS;
        (bytes.get(PREFIX.len()..end)?, end)
    };
    if digits.is_empty() {
        return None;
    }
    let mut value = 0u64;
    for (index, byte) in digits.iter().enumerate() {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + HEX_LETTER_OFFSET,
            b'A'..=b'F' => byte - b'A' + HEX_LETTER_OFFSET,
            _ => return None,
        };
        value += u64::from(digit) << (BITS_PER_HEX_DIGIT * (digits.len() - index - 1));
    }
    if value > MAXIMUM_SCALAR || SURROGATES.contains(&value) {
        return None;
    }
    Some((width, value as u32))
}

#[kani::proof]
#[kani::unwind(13)]
fn unicode_escape_prefixes_up_to_12_bytes_match_scalar_syntax() {
    let bytes: [u8; 12] = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= bytes.len());
    let prefix = &bytes[..length];
    let expected = specified_unicode_escape(prefix);
    let actual = tsx::unicode_escape(prefix).map(|escape| (escape.width(), escape.scalar() as u32));
    assert_eq!(
        actual, expected,
        "Unicode escape decoding matches the bounded scalar specification"
    );
    kani::cover!(actual.is_some());
    kani::cover!(actual.is_none());
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(13)]
fn reject_a_surrogate_identifier_escape() {
    assert!(
        tsx::unicode_escape(b"\\u{d800}").is_some(),
        "probe: accepting a surrogate identifier escape"
    );
}

#[kani::proof]
#[kani::should_panic]
#[kani::unwind(13)]
fn reject_an_out_of_range_identifier_escape() {
    assert!(
        tsx::unicode_escape(b"\\u{110000}").is_some(),
        "probe: accepting an out-of-range identifier escape"
    );
}
