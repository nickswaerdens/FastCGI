macro_rules! standard_record_types {
    (
        $(
            ($variant:ident, $num:expr);
        )+
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum Standard {
            $(
                $variant = $num,
            )+
        }

        impl From<u8> for Standard {
            fn from(value: u8) -> Self {
                match value {
                    $(
                        $num => Self::$variant,
                    )+
                    _ => Self::UnknownType
                }
            }
        }

        impl From<Standard> for u8 {
            fn from(value: Standard) -> Self {
                value as u8
            }
        }
    };
}

standard_record_types! {
    (BeginRequest, 1);
    (AbortRequest, 2);
    (EndRequest, 3);
    (Params, 4);
    (Stdin, 5);
    (Stdout, 6);
    (Stderr, 7);
    (Data, 8);
    (GetValues, 9);
    (GetValuesResult, 10);
    (UnknownType, 11);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    Standard(Standard),
    Custom(Custom),
}

impl From<u8> for RecordType {
    fn from(value: u8) -> Self {
        match value {
            1..=11 => Self::Standard(value.into()),
            _ => Self::Custom(value.into()),
        }
    }
}

impl From<RecordType> for u8 {
    fn from(value: RecordType) -> Self {
        match value {
            RecordType::Standard(std) => std as u8,
            RecordType::Custom(custom) => custom.record_type,
        }
    }
}

impl From<Standard> for RecordType {
    fn from(value: Standard) -> Self {
        RecordType::Standard(value)
    }
}

impl From<Custom> for RecordType {
    fn from(value: Custom) -> Self {
        RecordType::Custom(value)
    }
}

impl PartialEq<RecordType> for Standard {
    fn eq(&self, other: &RecordType) -> bool {
        RecordType::Standard(*self) == *other
    }
}

impl PartialEq<RecordType> for Custom {
    fn eq(&self, other: &RecordType) -> bool {
        RecordType::Custom(*self) == *other
    }
}

impl PartialEq<Standard> for RecordType {
    fn eq(&self, other: &Standard) -> bool {
        *self == RecordType::Standard(*other)
    }
}

impl PartialEq<Custom> for RecordType {
    fn eq(&self, other: &Custom) -> bool {
        *self == RecordType::Custom(*other)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Custom {
    record_type: u8,
}

impl Custom {
    pub fn new(n: u8) -> Self {
        assert!(n > 11);

        Self { record_type: n }
    }
}

impl From<u8> for Custom {
    fn from(value: u8) -> Self {
        Custom::new(value)
    }
}

impl From<Custom> for u8 {
    fn from(value: Custom) -> Self {
        value.record_type
    }
}
