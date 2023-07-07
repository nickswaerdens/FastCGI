use super::record::{Custom, RecordType};

mod private {
    use super::*;
    use crate::protocol::record::{
        AbortRequest, BeginRequest, Data, EndRequest, GetValues, GetValuesResult, Params, Stderr,
        Stdin, Stdout, UnknownType,
    };

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

    // Custom user records.
    impl<T: MetaCoreExt> Sealed for T {}

    // Meta types.
    impl Sealed for Application {}
    impl Sealed for Management {}

    impl Sealed for Discrete {}
    impl Sealed for Stream {}

    impl Sealed for Server {}
    impl Sealed for Client {}
}

/// Refer to `Meta` for the object-safe version of this trait.
pub trait MetaCore: private::Sealed {
    const TYPE: RecordType;
    type RecordKind: RecordKind;
    type DataKind: DataKind;
}

/// Object-safe version of `MetaCore`.
pub trait Meta: private::Sealed {
    fn record_type(&self) -> RecordType;
}

impl<T: MetaCore> Meta for T {
    fn record_type(&self) -> RecordType {
        T::TYPE
    }
}

/// `MetaCoreExt` records can only be of record kind `Management`.
/// Record types 0..=11 are reserved.
///
/// Currently an experimental trait.
pub trait MetaCoreExt {
    const TYPE: Custom;
    type DataKind: DataKind;
    type SentBy: SentBy;

    // The associated return type of a management record.
    type Dual;
}

// Implement `MetaCore` for extended record types.
impl<T: MetaCoreExt> MetaCore for T {
    const TYPE: RecordType = RecordType::Custom(T::TYPE);
    type RecordKind = Management;
    type DataKind = T::DataKind;
}

/// Object safe MetaCoreExt trait for requests.
pub trait DynRequestMetaExt: private::Sealed {}
impl<T: MetaCoreExt<SentBy = Client>> DynRequestMetaExt for T {}

/// Object safe MetaCoreExt trait for responses.
pub trait DynResponseMetaExt: private::Sealed {}
impl<T: MetaCoreExt<SentBy = Server>> DynResponseMetaExt for T {}

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

/// Specifies whether the record is sent by a `Server` or `Client`.
/// `Client (BeginRequest...) -> Server (...EndRequest) -> Client`
pub trait SentBy: private::Sealed {
    type Dual: SentBy;
}

pub enum Server {}
pub enum Client {}

impl SentBy for Server {
    type Dual = Client;
}

impl SentBy for Client {
    type Dual = Server;
}
