use std::ops::Deref;

pub enum RO<'a, T> {
    Ref(&'a T),
    Owned(T),
}

impl<'a, T> Deref for RO<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            RO::Ref(r) => *r,
            RO::Owned(r) => r,
        }
    }
}
