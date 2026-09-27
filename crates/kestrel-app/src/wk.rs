//! Weak-reference plumbing for glib closures over plain-Rc app structs.
//!
//! `glib::clone!` requires `Downgrade`/`Upgrade`. The `Upgrade` impl cannot
//! be written for foreign `std::rc::Weak`, so we wrap it in a local newtype.

pub struct WeakBox<T>(pub std::rc::Weak<T>);

impl<T> glib::clone::Upgrade for WeakBox<T> {
    type Strong = std::rc::Rc<T>;
    fn upgrade(&self) -> Option<Self::Strong> {
        self.0.upgrade()
    }
}

/// Implement `glib::clone::Downgrade` for an `Rc`-shared local type.
#[macro_export]
macro_rules! impl_rc_downgrade {
    ($t:ty) => {
        impl glib::clone::Downgrade for $t {
            type Weak = $crate::wk::WeakBox<$t>;
            fn downgrade(&self) -> Self::Weak {
                $crate::wk::WeakBox(std::rc::Rc::downgrade(self))
            }
        }
    };
}
