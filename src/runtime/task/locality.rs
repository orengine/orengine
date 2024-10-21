#[cfg(any(target_pointer_width = "64"))]
pub(crate) const IS_LOCAL_SHIFT: i128 = 127;
#[cfg(any(target_pointer_width = "64"))]
pub(crate) const TASK_MASK: i128 = !(1 << IS_LOCAL_SHIFT);
#[cfg(any(target_pointer_width = "64"))]
pub(crate) const IS_LOCAL_MASK: i128 = 1 << IS_LOCAL_SHIFT;

pub struct Locality {
    #[cfg(not(target_pointer_width = "64"))]
    pub(crate) value: bool,
    #[cfg(any(target_pointer_width = "64"))]
    pub(crate) value: i128,
}

impl Locality {
    #[inline(always)]
    pub fn local() -> Self {
        #[cfg(not(target_pointer_width = "64"))]
        {
            Self { value: true }
        }

        #[cfg(any(target_pointer_width = "64"))]
        Self {
            value: IS_LOCAL_MASK,
        }
    }

    #[inline(always)]
    pub fn global() -> Self {
        #[cfg(not(target_pointer_width = "64"))]
        {
            Self { value: false }
        }

        #[cfg(any(target_pointer_width = "64"))]
        Self { value: 0 }
    }

    #[inline(always)]
    pub fn is_local(&self) -> bool {
        #[cfg(not(target_pointer_width = "64"))]
        {
            self.value
        }

        #[cfg(any(target_pointer_width = "64"))]
        {
            self.value > 0
        }
    }
}
