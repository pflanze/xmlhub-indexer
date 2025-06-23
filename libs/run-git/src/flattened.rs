/// Create a new vector that contains owned versions of the elements
/// of all vectors/slices. Comes in two variants, into* consuming the
/// input, and one using ToOwned instead.

pub trait IntoFlattened<T> {
    fn into_flattened(self) -> Vec<T>;
}

impl<T> IntoFlattened<T> for Vec<Vec<T>> {
    fn into_flattened(self) -> Vec<T> {
        let mut output = Vec::new();
        for mut vec in self {
            output.append(&mut vec);
        }
        output
    }
}

pub trait Flattened<T: ToOwned<Owned = U>, U> {
    fn flattened(self) -> Vec<U>;
}

impl<T: ToOwned<Owned = U>, U, V: AsRef<[T]>> Flattened<T, U> for &[V] {
    fn flattened(self) -> Vec<U> {
        let mut output: Vec<U> = Vec::new();
        for v in self {
            for item in v.as_ref() {
                output.push(item.to_owned());
            }
        }
        output
    }
}
