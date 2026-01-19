/// Evaluate a set of explicit yes and explicit no boolean values to a
/// single boolean, given a default yes or no.
pub trait EvalWithDefault {
    fn explicit_yes_and_no(&self) -> (bool, bool);

    fn eval_with_default(&self, default: bool) -> bool {
        match self.explicit_yes_and_no() {
            (true, false) => true,
            (false, true) => false,
            (false, false) | (true, true) => default,
        }
    }
}
