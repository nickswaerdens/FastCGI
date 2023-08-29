pub(crate) mod defrag;
pub(crate) mod request;
pub(crate) mod response;

#[macro_export]
macro_rules! build_enum_with_from_impls {
    (
        $vis:vis $name:ident {
            $($variant:tt $(($struct:ty))?,)*
        }
    ) => {
        #[derive(Debug)]
        $vis enum $name {
            $($variant $(($struct))?,)*
        }

        macro_rules! impl_from {
            ($inner:tt $frame:ty) => {
                impl From<$frame> for $name {
                    fn from(value: $frame) -> Self {
                        $name::$inner(value)
                    }
                }

                impl TryFrom<$name> for $frame {
                    type Error = $name;

                    fn try_from(kind: $name) -> Result<Self, Self::Error> {
                        match kind {
                            $name::$inner(frame) => Ok(frame),
                            e => Err(e),
                        }
                    }
                }
            };
            ($inner:tt) => {
                // Do nothing as `From` cannot be implemented for unit-like enum variants.
            };
        }

        $(
            impl_from!($variant $($struct)?);
        )*
    }
}
