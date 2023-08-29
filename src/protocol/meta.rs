use super::record::{ManagementRecordType, RecordType};

mod private {
    use super::*;
    use crate::protocol::record::{
        AbortRequest, BeginRequest, Data, EndRequest, Params, Stderr, Stdin, Stdout,
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

    // Management records.
    impl<T: ManagementRecord> Sealed for T {}

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

/// Record types 0..=11 are reserved.
///
/// Currently an experimental trait.
pub trait ManagementRecord {
    const TYPE: ManagementRecordType;
    type DataKind: DataKind;
    type Endpoint: Endpoint;

    // The associated return type of a management record.
    type Dual;
}

pub trait DynManagementRecord {
    fn record_type() -> ManagementRecordType;
}

impl<T> DynManagementRecord for T
where
    T: ManagementRecord,
{
    fn record_type() -> ManagementRecordType {
        T::TYPE
    }
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

// Implement `MetaCore` for extended record types.
impl<T: ManagementRecord> MetaCore for T {
    const TYPE: RecordType = RecordType::Management(T::TYPE);
    type RecordKind = Management;
    type DataKind = T::DataKind;
}

/*
/// Object safe MetaCoreExt trait for requests.
pub trait DynRequestMetaExt: private::Sealed {}
impl<T: MetaCoreExt<SentBy = Client>> DynRequestMetaExt for T {}

/// Object safe MetaCoreExt trait for responses.
pub trait DynResponseMetaExt: private::Sealed {}
impl<T: MetaCoreExt<SentBy = Server>> DynResponseMetaExt for T {}
*/

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
pub trait Endpoint: private::Sealed {
    type Dual: Endpoint;
}

pub enum Client {}
pub enum Server {}

impl Endpoint for Client {
    type Dual = Server;
}

impl Endpoint for Server {
    type Dual = Client;
}
