use super::defrag::{Defrag, MaximumStreamSizeExceeded};
use crate::{
    build_enum_with_from_impls,
    protocol::{
        record::{
            begin_request::Role, ApplicationRecordType, BeginRequest, Data, Decode, DecodeError,
            Params, RecordType, Stdin,
        },
        transport::Frame,
    },
};

type ParseResult<T> = Result<T, ParseRequestError>;

fn validate_record_type(lh: RecordType, rh: impl PartialEq<RecordType>) -> ParseResult<()> {
    (rh == lh)
        .then_some(())
        .ok_or(ParseRequestError::UnexpectedRecordType(lh))
}

#[derive(Debug, Default, Clone, Copy)]
enum State {
    #[default]
    BeginRequest,
    Params(Role),
    Stdin(Role),
    Data,
    Finished,
    Aborted,
}

#[derive(Debug)]
pub(crate) enum Transition {
    Parse(Frame),
    EndOfStream(RecordType),
    Abort,
}

impl Transition {
    pub(crate) fn parse(frame: Frame) -> Transition {
        let (id, record_type, payload) = frame.as_parts();

        assert!(id > 0);

        if !payload.is_empty() {
            Transition::Parse(frame)
        } else if record_type == ApplicationRecordType::AbortRequest {
            Transition::Abort
        } else {
            Transition::EndOfStream(record_type)
        }
    }
}

#[derive(Debug)]
pub(crate) struct Parser {
    inner: State,
    defrag: Defrag,
}

impl Parser {
    pub(crate) fn new(max_total_payload: usize) -> Self {
        Parser {
            inner: State::BeginRequest,
            defrag: Defrag::new(max_total_payload),
        }
    }

    /// Return a Part when it can be fully constructed, otherwise returns None.
    pub(crate) fn parse_frame(&mut self, transition: Transition) -> ParseResult<Option<Part>> {
        let part = match (self.inner, transition) {
            (State::BeginRequest, Transition::Parse(frame)) => {
                let (_, record_type, payload) = frame.into_parts();

                validate_record_type(record_type, ApplicationRecordType::BeginRequest)?;

                let begin_request = BeginRequest::decode(payload)?;

                self.inner = State::Params(begin_request.get_role());

                Some(Part::from(begin_request))
            }

            (State::Params(_), Transition::Parse(frame)) => {
                let (_, record_type, payload) = frame.into_parts();

                validate_record_type(record_type, ApplicationRecordType::Params)?;

                self.defrag.insert_payload(payload)?;

                None
            }
            (State::Params(role), Transition::EndOfStream(record_type)) => {
                validate_record_type(record_type, ApplicationRecordType::Params)?;

                let payload = self.defrag.handle_end_of_stream();

                if payload.is_empty() {
                    return Err(ParseRequestError::ParamsMustBeLargerThanZero);
                }

                let params = Params::decode(payload)?;

                self.inner = State::Stdin(role);

                Some(Part::from(params))
            }

            (State::Stdin(_), Transition::Parse(frame)) => {
                let (_, record_type, payload) = frame.into_parts();

                validate_record_type(record_type, ApplicationRecordType::Stdin)?;

                self.defrag.insert_payload(payload)?;

                None
            }
            (State::Stdin(role), Transition::EndOfStream(record_type)) => {
                validate_record_type(record_type, ApplicationRecordType::Stdin)?;

                let payload = self.defrag.handle_end_of_stream();
                let stdin = (!payload.is_empty())
                    .then_some(Stdin::decode(payload))
                    .transpose()?;

                self.inner = match role {
                    Role::Filter => State::Data,
                    _ => State::Finished,
                };

                Some(Part::from(stdin))
            }

            (State::Data, Transition::Parse(frame)) => {
                let (_, record_type, payload) = frame.into_parts();

                validate_record_type(record_type, ApplicationRecordType::Data)?;

                self.defrag.insert_payload(payload)?;

                None
            }
            (State::Data, Transition::EndOfStream(record_type)) => {
                validate_record_type(record_type, ApplicationRecordType::Data)?;

                let payload = self.defrag.handle_end_of_stream();

                if payload.is_empty() {
                    return Err(ParseRequestError::DataIsRequiredForFilterApplications);
                }

                let data = Data::decode(payload)?;

                self.inner = State::Finished;

                Some(Part::from(data))
            }

            // Abort
            (_, Transition::Abort) => {
                self.inner = State::Aborted;

                Some(Part::AbortRequest)
            }

            // Errors
            (State::Finished, _) => return Err(ParseRequestError::InvalidParser),

            (_, Transition::EndOfStream(record_type)) => {
                return Err(ParseRequestError::UnexpectedRecordType(record_type))
            }
            (_, Transition::Parse(frame)) => {
                return Err(ParseRequestError::UnexpectedRecordType(frame.record_type))
            }
        };

        Ok(part)
    }
}

build_enum_with_from_impls! {
    pub(crate) Part {
        BeginRequest(BeginRequest),
        AbortRequest,
        Params(Params),
        Stdin(Option<Stdin>),
        Data(Data),
    }
}

#[derive(Debug)]
pub enum ParseRequestError {
    InvalidParser,
    UnexpectedRecordType(RecordType),

    // Specific errors.
    ParamsMustBeLargerThanZero,
    DataIsRequiredForFilterApplications,

    // Defrag
    MaximumStreamSizeExceeded(MaximumStreamSizeExceeded),

    DecodeError(DecodeError),
    StdIoError(std::io::Error),
}

impl From<std::io::Error> for ParseRequestError {
    fn from(value: std::io::Error) -> Self {
        ParseRequestError::StdIoError(value)
    }
}

impl From<DecodeError> for ParseRequestError {
    fn from(value: DecodeError) -> Self {
        ParseRequestError::DecodeError(value)
    }
}

impl From<MaximumStreamSizeExceeded> for ParseRequestError {
    fn from(value: MaximumStreamSizeExceeded) -> Self {
        ParseRequestError::MaximumStreamSizeExceeded(value)
    }
}
