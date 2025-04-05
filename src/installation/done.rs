use std::fmt::Display;

/// A report on what was done, for verbose output.
#[derive(Debug)]
pub enum Done {
    None,
    Some { previously: Box<Done>, now: String },
}

impl From<String> for Done {
    fn from(now: String) -> Self {
        Done::Some {
            previously: Done::None.into(),
            now,
        }
    }
}

impl From<&str> for Done {
    fn from(now: &str) -> Self {
        Done::Some {
            previously: Done::None.into(),
            now: now.into(),
        }
    }
}

impl Display for Done {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Done::None => Ok(()),
            Done::Some { previously, now } => {
                previously.fmt(f)?;
                f.write_fmt(format_args!("* {}\n", now))
            }
        }
    }
}

impl Done {
    pub fn is_none(&self) -> bool {
        match self {
            Done::None => true,
            Done::Some {
                previously: _,
                now: _,
            } => false,
        }
    }

    pub fn nothing() -> Done {
        Done::None
    }
    pub fn with_previously(self, older: Self) -> Self {
        match self {
            Done::None => older,
            Done::Some { previously, now } => {
                if older.is_none() {
                    Self::Some { previously, now }
                } else {
                    let prev_with_older = previously.with_previously(older);
                    Self::Some {
                        previously: prev_with_older.into(),
                        now,
                    }
                }
            }
        }
    }
}

#[test]
fn t_done() {
    let done1 = Done::nothing();
    let done2 = Done::from("done2");
    let done3 = Done::from("done3");
    let done4 = Done::from("done4");
    let alldone = done4.with_previously(done3.with_previously(done2.with_previously(done1)));
    assert_eq!(alldone.to_string(), "* done2\n* done3\n* done4\n");
}

#[test]
fn t_done_tree() {
    let done1 = Done::from("done1");
    let done2 = Done::from("done2").with_previously(done1);
    let done3 = Done::from("done3");
    let done4 = Done::from("done4").with_previously(done3);
    let alldone = done4.with_previously(done2);
    assert_eq!(alldone.to_string(), "* done1\n* done2\n* done3\n* done4\n");
}
