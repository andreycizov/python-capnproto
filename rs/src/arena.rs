use std::rc::Rc;
use std::ops::{Deref, DerefMut};

pub trait Arena: Sized {
    type Item: ?Sized;

    fn get(&self, idx: &u64) -> Option<&Self::Item>;
}

pub struct ArenaRc<A: Arena>
{
    inner: Rc<A>,
}

pub struct ArenaRef<A: Arena>
{
    idx: u64,
    arena: Rc<A>,
}

impl<A: Arena> Clone for ArenaRef<A> {
    fn clone(&self) -> Self {
        ArenaRef { idx: self.idx, arena: self.arena.clone() }
    }
}

impl<A: Arena> From<A> for ArenaRc<A>
// there's more than 1 arena that can be Arena<Item=T>
{
    fn from(a: A) -> ArenaRc<A> {
        return ArenaRc { inner: Rc::new(a) }
    }
}

impl<A: Arena> From<&Rc<A>> for ArenaRc<A>
// there's more than 1 arena that can be Arena<Item=T>
{
    fn from(a: &Rc<A>) -> ArenaRc<A> {
        return ArenaRc { inner: a.clone() }
    }
}

impl<A: Arena> ArenaRc<A> {
    pub fn get_ref(&self, idx: &u64) -> Option<ArenaRef<A>> {
        self.inner.get(idx).map(|_| ArenaRef { idx: idx.clone(), arena: self.inner.clone() })
    }
}

impl<A: Arena> Deref for ArenaRc<A> {
    type Target = A;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl <A: Arena>DerefMut for ArenaRc<A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::rc::Rc::<A>::get_mut(&mut self.inner).unwrap()
    }
}

impl <A: Arena>ArenaRef<A> {
    fn from_rc(arena: Rc<A>, idx: &u64) -> Option<Self> {
        arena.get(idx).map(|_| Self { idx: *idx, arena: arena.clone() })
    }

    pub fn arena(&self) -> ArenaRc<A> {
        return ArenaRc::from(&self.arena)
    }

    pub fn get(&self) -> Option<&A::Item> {
        self.arena.get(&self.idx)
    }
}

impl <A: Arena>Deref for ArenaRef<A> {
    type Target = A::Item;

    fn deref(&self) -> &Self::Target {
        &self.get().unwrap()
    }
}