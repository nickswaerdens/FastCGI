#[macro_export]
macro_rules! await_variant {
    ($connection:ident, Part::$variant:ident) => {{
        loop {
            if let Some(result) = $connection.poll_frame().await {
                match result? {
                    Part::$variant(inner) => {
                        break inner;
                    }
                    Part::AbortRequest => {
                        // TODO: Handle aborted request on the connection.
                        $connection.close_stream();

                        return Ok(None);
                    }
                    _ => unreachable!(),
                }
            }
        }
    }};
}

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

/// Implements the `Meta` trait for standard record types.
#[macro_export]
macro_rules! impl_std_meta {
    // Slightly adjusted from: https://stackoverflow.com/a/61189128.
    // Doesn't support module paths nor 'where' constraints.
    (
        $(
            ($variant:ident $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)?, $rkind:ident, $dkind:ident);
        )+
    ) => {
        $(
            impl $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? Meta for $variant $(< $( $lt ),+ >)?
            {
                const TYPE: RecordType = RecordType::Standard(Standard::$variant);
                type RecordKind = $rkind;
                type DataKind = $dkind;
            }
        )+
    }
}
