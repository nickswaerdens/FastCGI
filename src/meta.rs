use crate::record::{Custom, RecordType};

mod private {
    use crate::record::{
        AbortRequest, BeginRequest, Data, EndOfStream, EndRequest, GetValues, GetValuesResult,
        Params, Stderr, Stdin, Stdout, UnknownType,
    };

    use super::*;

    pub trait Sealed {}

    // Standard records.
    impl Sealed for BeginRequest {}
    impl Sealed for AbortRequest {}
    impl Sealed for EndRequest {}
    impl Sealed for Params {}
    impl Sealed for Stdin {}
    impl Sealed for Data {}
    impl Sealed for Stdout {}
    impl Sealed for Stderr {}
    impl Sealed for GetValues {}
    impl Sealed for GetValuesResult {}
    impl Sealed for UnknownType {}

    // EndOfStream stream records.
    impl<T: Meta<DataKind = Stream>> Sealed for EndOfStream<T> {}

    // Custom user records.
    impl<T: MetaExt> Sealed for T {}

    // Meta types.
    impl Sealed for Server {}
    impl Sealed for Client {}

    impl Sealed for Application {}
    impl Sealed for Management {}

    impl Sealed for Discrete {}
    impl Sealed for Stream {}
}

pub trait Meta: private::Sealed {
    const TYPE: RecordType;
    type SentBy: SentBy;
    type RecordKind: RecordKind;
    type DataKind: DataKind;
}

/// Trait which allows `Box<dyn DynMeta>` = `Box<dyn Meta<...>>` without specifying associated types.
pub trait DynRequestMetaExt: private::Sealed {}
impl<T: MetaExt<SentBy = Client>> DynRequestMetaExt for T {}

pub trait DynResponseMetaExt: private::Sealed {}
impl<T: MetaExt<SentBy = Server>> DynResponseMetaExt for T {}

/// Specifies whether the record is sent by a `Server` or `Client`.
/// `Client (BeginRequest...) -> Server (...EndRequest) -> Client`
pub trait SentBy: private::Sealed {}
pub enum Server {}
pub enum Client {}
impl SentBy for Server {}
impl SentBy for Client {}

/// Specifies whether the record is a `Management` or `Application` type.
pub trait RecordKind: private::Sealed {}
pub enum Application {}
pub enum Management {}
impl RecordKind for Application {}
impl RecordKind for Management {}

/// Specifies whether the record is a `Discrete` or `Stream` type.
pub trait DataKind: private::Sealed {}
pub enum Discrete {}
pub enum Stream {}
impl DataKind for Discrete {}
impl DataKind for Stream {}

/// `MetaExt` records can only be of record kind `Management`.
/// Record types 0..=11 are reserved, and will result in the management record being ignored by client and server applications.
pub trait MetaExt {
    const TYPE: Custom;
    type SentBy: SentBy;
    type DataKind: DataKind;
    type Dual; //: MetaExt + DecodeFrame;
}

// Implement `Meta` for extended record types.
impl<T: MetaExt> Meta for T {
    const TYPE: RecordType = RecordType::Custom(T::TYPE);
    type SentBy = T::SentBy;
    type RecordKind = Management;
    type DataKind = T::DataKind;
}
