macro_rules! application_record_types {
    (
        $(
            ($variant:ident, $num:expr);
        )+
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum ApplicationRecordType {
            $(
                $variant = $num,
            )+
        }


        impl From<u8> for ApplicationRecordType {
            fn from(value: u8) -> Self {
                match value {
                    $(
                        $num => Self::$variant,
                    )+
                    _ => unreachable!()
                }
            }
        }


        impl From<ApplicationRecordType> for u8 {
            fn from(value: ApplicationRecordType) -> Self {
                value as u8
            }
        }
    };
}

application_record_types! {
    (BeginRequest, 1);
    (AbortRequest, 2);
    (EndRequest, 3);
    (Params, 4);
    (Stdin, 5);
    (Stdout, 6);
    (Stderr, 7);
    (Data, 8);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    Application(ApplicationRecordType),
    Management(ManagementRecordType),
}

impl From<u8> for RecordType {
    fn from(value: u8) -> Self {
        match value {
            1..=8 => Self::Application(value.into()),
            _ => Self::Management(value.into()),
        }
    }
}

impl From<RecordType> for u8 {
    fn from(value: RecordType) -> Self {
        match value {
            RecordType::Application(std) => std as u8,
            RecordType::Management(custom) => custom.record_type,
        }
    }
}

impl From<ApplicationRecordType> for RecordType {
    fn from(value: ApplicationRecordType) -> Self {
        RecordType::Application(value)
    }
}

impl From<ManagementRecordType> for RecordType {
    fn from(value: ManagementRecordType) -> Self {
        RecordType::Management(value)
    }
}

impl PartialEq<RecordType> for ApplicationRecordType {
    fn eq(&self, other: &RecordType) -> bool {
        RecordType::Application(*self) == *other
    }
}

impl PartialEq<RecordType> for ManagementRecordType {
    fn eq(&self, other: &RecordType) -> bool {
        RecordType::Management(*self) == *other
    }
}

impl PartialEq<ApplicationRecordType> for RecordType {
    fn eq(&self, other: &ApplicationRecordType) -> bool {
        *self == RecordType::Application(*other)
    }
}

impl PartialEq<ManagementRecordType> for RecordType {
    fn eq(&self, other: &ManagementRecordType) -> bool {
        *self == RecordType::Management(*other)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagementRecordType {
    record_type: u8,
}

impl ManagementRecordType {
    pub const fn new(n: u8) -> Self {
        assert!(n > 11);

        Self::new_unchecked(n)
    }

    pub(crate) const fn new_unchecked(n: u8) -> Self {
        Self { record_type: n }
    }
}

impl From<u8> for ManagementRecordType {
    fn from(value: u8) -> Self {
        ManagementRecordType::new(value)
    }
}

impl From<ManagementRecordType> for u8 {
    fn from(value: ManagementRecordType) -> Self {
        value.record_type
    }
}
