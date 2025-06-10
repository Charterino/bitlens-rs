use std::borrow::Cow;

// Huge thanks to @goldsteinq and @variant for their help with figuring out how to do this!
// Literally wouldn't be able to figure this out without their help!!!! They're awesome!!!!!!!!!
pub trait MustOutlive<'old> {
    type WithLifetime<'new: 'old>: Clone;
}
pub trait DeepClone<'old, 'new: 'old>: MustOutlive<'old> {
    fn deep_clone(&self) -> Self::WithLifetime<'new>;
}

impl<'old, T: Clone + MustOutlive<'old>> MustOutlive<'old> for &[T] {
    type WithLifetime<'new: 'old> = Vec<T::WithLifetime<'new>>;
}

impl<'old, 'new: 'old, T: Clone + DeepClone<'old, 'new>> DeepClone<'old, 'new> for &[T] {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        let mut res = Vec::with_capacity(self.len());
        for i in 0..self.len() {
            let item = self.get(i).unwrap();
            res.push(item.deep_clone());
        }
        res
    }
}

impl<'old, T: Clone + MustOutlive<'old>> MustOutlive<'old> for Cow<'old, T> {
    type WithLifetime<'new: 'old> = T::WithLifetime<'new>;
}

impl<'old, 'new: 'old, T: Clone + DeepClone<'old, 'new>> DeepClone<'old, 'new> for Cow<'old, T> {
    fn deep_clone(&self) -> Self::WithLifetime<'new> {
        (**self).deep_clone()
    }
}
