//! Abstraction for effects used by build processes.

//! The aim is to use structs that contain pre-evaluated context data
//! and can show it (one reason why `Debug` is required by `Effect`)
//! before executing them, and then also have them specify
//! dependencies and outputs (only one type for each, but one could
//! use tuples? -> FUTURE work)).

use std::{any::type_name, fmt::Debug, marker::PhantomData};

use anyhow::Result;

fn strip_namespace(s: &str) -> &str {
    s.split("::").last().unwrap()
}

fn show_arrow<T>() -> String {
    format!(
        "\n    |\n    | {}\n    v\n",
        strip_namespace(type_name::<T>())
    )
}

/// An effect must specify its requirements--the result(s) provided
/// from running other `Effect`(s)--and what it provides (which can
/// then be used to run other `Effect`s).
pub trait Effect: Debug {
    type Requires;
    type Provides;

    /// Show as a string for simplicity, as multiple lines with
    /// trailing newline.
    fn show(&self) -> String {
        format!("{:#?}{}", self, show_arrow::<Self::Provides>())
    }

    /// Carry out the effect of this `Effect`. Using Box to allow for
    /// dyn (an alternative might be to use the `auto_enums` crate
    /// instead?)
    fn run(self: Box<Self>, provided: Self::Requires) -> Result<Self::Provides>;

    // Can't get this to work currently because it has Self in the
    // return value; see `bind` function instead.
    // fn then<P, E2: Effect<Requires = Self::Provides, Provides = P> + ?Sized>(
    //     self: Box<Self>,
    //     e2: Box<E2>,
    // ) -> Binding<Self::Requires, Self::Provides, P, Self, E2>
    // {
    //     Binding { e1: self, e2 }
    // }
}

/// Binding two effects into a sequence. Using Box to allow for dyn
/// (an alternative might be to use the `auto_enums` crate instead?)
pub fn bind<
    R,
    PI,
    P,
    E1: Effect<Requires = R, Provides = PI> + ?Sized,
    E2: Effect<Requires = PI, Provides = P> + ?Sized,
>(
    e1: Box<E1>,
    e2: Box<E2>,
) -> Box<Seq<R, PI, P, E1, E2>> {
    Box::new(Seq(e1, e2))
}

/// Representation of a combined effect of two other effects,
/// sequencing them for execution. See `bind` for easier creation.
#[derive(Debug)]
pub struct Seq<
    R,
    PI,
    P,
    E1: Effect<Requires = R, Provides = PI> + ?Sized,
    E2: Effect<Requires = PI, Provides = P> + ?Sized,
>(Box<E1>, Box<E2>);

// (Why does this need Debug on the intermediate types? aha, for
// "?". Or is it deeper?)
impl<
        R: Debug,
        PI: Debug,
        P: Debug,
        E1: Effect<Requires = R, Provides = PI> + ?Sized,
        E2: Effect<Requires = PI, Provides = P> + ?Sized,
    > Effect for Seq<R, PI, P, E1, E2>
{
    type Requires = R;
    type Provides = P;

    fn show(&self) -> String {
        format!("{}\n{}", self.0.show(), self.1.show())
    }

    fn run(self: Box<Self>, provided: Self::Requires) -> Result<Self::Provides> {
        let pi = self.0.run(provided)?;
        self.1.run(pi)
    }
}

/// An effect that provides `P` without doing any work, as stand-in
/// for places where work is optional. I.e. converts `R` to `P`
/// without any side effect.
#[derive(Debug)]
pub struct NoOp<R, P> {
    phantom: PhantomData<fn() -> R>,
    providing: P,
    why: &'static str,
}

impl<R, P> NoOp<R, P> {
    pub fn providing(providing: P, why: &'static str) -> Box<Self> {
        Box::new(Self {
            phantom: PhantomData,
            providing,
            why,
        })
    }
}

impl<R: Debug, P: Debug> Effect for NoOp<R, P> {
    type Requires = R;
    type Provides = P;

    fn show(&self) -> String {
        format!(
            "NoOp providing {:?}: {}{}",
            self.providing,
            self.why,
            show_arrow::<Self::Provides>()
        )
    }

    fn run(self: Box<Self>, _provided: Self::Requires) -> Result<Self::Provides> {
        Ok(self.providing)
    }
}
