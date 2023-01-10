use std::collections::HashMap;

use core::iter::once;
use core::mem;

include!("shared.rs");
include!(concat!(env!("OUT_DIR"), "/tables.rs"));

/// Returns newlines where this text needs it.
pub fn apply_newlines(
    string: &str,
    max_width: usize,
    font: &HashMap<char, usize>,
) -> Result<String, LineBreakErr> {
    // Set up our output string and retrieve our linebreak information
    let mut output = String::new();
    let mut breakers = linebreaks(string);

    let mut chars: Vec<(char, Option<BreakOpportunity>)> = string
        .char_indices()
        .map(|(_, c)| (c, breakers.next().expect("linebreak issue in `inner`").1))
        .collect();

    // Iterate over our input until
    // we have successfully processed the whole thing.
    loop {
        let mut current_width = 0;
        let mut break_point: Option<usize> = None;
        let mut applied_line_break = false;

        for (cursor, (c, break_op)) in chars.iter().enumerate() {
            // Break on null terminator -- we probably shouldn't find any of these...
            if *c == '\0' {
                break;
            }

            // Reset on newlines
            if *c == '\n' {
                current_width = 0;
                continue;
            }

            // Add the width of this character
            current_width += font.get(c).ok_or(LineBreakErr::MissingCharacterWidth(*c))?;

            // Are we over the max width now? If so, create a linebreak at our last
            // safe break point
            if current_width > max_width {
                if let Some(break_point) = break_point {
                    use std::fmt::Write;

                    // Create the split
                    let (prefix, postfix) = chars.split_at(break_point);
                    let prefix: String = prefix.iter().map(|&(c, _)| c).collect();
                    writeln!(output, "{}", prefix).unwrap();

                    // We will now modify chars so that if we need to run again, we will only be
                    // iterating on the unprocessed characters.
                    chars = postfix.to_vec();
                    applied_line_break = true;
                    break;
                } else {
                    return Err(LineBreakErr::NoLegalLinebreakOpportunity);
                }
            }

            // We weren't over the limit, so we can continue -- but if this is a safe
            // break point, let's remember that
            if break_op.is_some() && cursor != 0 {
                break_point = Some(cursor);
            }
        }

        // Once we're here, we check if we made it to the end of our characters.
        // If we did, we're done!
        if !applied_line_break {
            break;
        }
    }

    // push in the final characters into the str
    let s: String = chars.into_iter().map(|n| n.0).collect();
    output.push_str(&s); // pushing the last bit in!
    Ok(output)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone, Copy)]
pub enum LineBreakErr {
    #[error("missing character width for `{0}`")]
    MissingCharacterWidth(char),
    #[error("no legal linebreak opportunity found")]
    NoLegalLinebreakOpportunity,
}

fn break_property(codepoint: u32) -> BreakClass {
    let codepoint = codepoint as usize;
    match PAGE_INDICES.get(codepoint >> 8) {
        Some(&page_idx) if page_idx & UNIFORM_PAGE != 0 => unsafe {
            mem::transmute((page_idx & !UNIFORM_PAGE) as u8)
        },
        Some(&page_idx) => BREAK_PROP_DATA[page_idx][codepoint & 0xFF],
        None => BreakClass::Unknown,
    }
}

/// Break opportunity type.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum BreakOpportunity {
    /// A line must break at this spot.
    Mandatory,
    /// A line is allowed to end at this spot.
    Allowed,
}

/// Returns an iterator over line break opportunities in the specified string.
fn linebreaks(s: &str) -> impl Iterator<Item = (usize, Option<BreakOpportunity>)> + Clone + '_ {
    use BreakOpportunity::{Allowed, Mandatory};

    s.char_indices()
        .map(|(i, c)| (i, break_property(c as u32) as u8))
        .chain(once((s.len(), eot)))
        .scan((sot, false), |state, (i, cls)| {
            // ZWJ is handled outside the table to reduce its size
            let val = PAIR_TABLE[state.0 as usize][cls as usize];
            let is_mandatory = val & MANDATORY_BREAK_BIT != 0;
            let is_break = val & ALLOWED_BREAK_BIT != 0 && (!state.1 || is_mandatory);
            *state = (
                val & !(ALLOWED_BREAK_BIT | MANDATORY_BREAK_BIT),
                cls == BreakClass::ZeroWidthJoiner as u8,
            );

            Some((i, is_break, is_mandatory))
        })
        .map(|(i, is_break, is_mandatory)| {
            if is_break {
                (i, Some(if is_mandatory { Mandatory } else { Allowed }))
            } else {
                (i, None)
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_font() -> HashMap<char, usize> {
        (0..=255)
            .filter_map(char::from_u32)
            .map(|c| (c, 1))
            .collect()
    }

    #[test]
    fn basic() {
        assert_eq!(
            apply_newlines("This is a simple newline string.", 35, &make_font()).unwrap(),
            "This is a simple newline string."
        );

        assert_eq!(
            apply_newlines(
                "This is a simple newline string. But then it gets a little longer.",
                35,
                &make_font()
            )
            .unwrap(),
            "This is a simple newline string. \nBut then it gets a little longer."
        );

        assert_eq!(
            apply_newlines("Supercalifragalisticexpialidocious", 30, &make_font()).unwrap_err(),
            LineBreakErr::NoLegalLinebreakOpportunity
        );

        assert_eq!(
            apply_newlines("≤", 30, &make_font()).unwrap_err(),
            LineBreakErr::MissingCharacterWidth('≤')
        );
    }
}
