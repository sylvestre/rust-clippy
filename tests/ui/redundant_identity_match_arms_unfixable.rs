//@no-rustfix: applying the suggestions would delete the comments
#![warn(clippy::redundant_identity_match_arms)]
#![allow(clippy::needless_match)]

// A comment inside a removed arm downgrades the suggestion to keep rustfix from
// deleting it.
fn comment_in_identity_arm(opt: Option<u32>) -> Option<u32> {
    match opt {
        Some(0) => None,
        Some(x) => {
            // Identity, wrapped in a block.
            Some(x)
        },
        //~^^^^ redundant_identity_match_arms
        None => None,
    }
}

// Same for a comment between a removed arm and the next one.
fn comment_between_arms(res: Result<u32, String>) -> Result<u32, String> {
    match res {
        Ok(v) => Ok(v),
        //~^ redundant_identity_match_arms
        // Normalize empty error messages.
        Err(s) if s.is_empty() => Err(String::from("empty")),
        Err(s) => Err(s),
    }
}

fn main() {}
