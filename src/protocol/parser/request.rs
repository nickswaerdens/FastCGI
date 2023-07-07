use super::defrag::{Defrag, MaximumStreamSizeExceeded};
use crate::{
    build_enum_with_from_impls,
    protocol::{
        record::{
            begin_request::Role, BeginRequest, Data, Decode, DecodeError, Params, RecordType,
            Standard, Stdin,
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
    Params,
    Stdin,
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
        } else if record_type == Standard::AbortRequest {
            Transition::Abort
        } else {
            Transition::EndOfStream(record_type)
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Parser {
    inner: State,
    role: Option<Role>,
    defrag: Defrag,
}

impl Parser {
    pub(crate) fn new() -> Self {
        Parser {
            inner: State::BeginRequest,
            role: None,
            defrag: Defrag::new(),
        }
    }

    /// Return a Part when it can be fully constructed, otherwise returns None.
    pub(crate) fn parse_frame(&mut self, transition: Transition) -> ParseResult<Option<Part>> {
        let part = match (self.inner, transition) {
            (State::BeginRequest, Transition::Parse(frame)) => {
                let (_id, record_type, payload) = frame.into_parts();

                validate_record_type(record_type, Standard::BeginRequest)?;

                let begin_request = BeginRequest::decode(payload)?;

                self.role = Some(begin_request.get_role());
                self.inner = State::Params;

                Some(Part::from(begin_request))
            }

            (State::Params, Transition::Parse(frame)) => {
                let (_, record_type, payload) = frame.into_parts();

                validate_record_type(record_type, Standard::Params)?;

                self.defrag.insert_payload(payload)?;

                None
            }
            (State::Params, Transition::EndOfStream(record_type)) => {
                validate_record_type(record_type, Standard::Params)?;

                let params = self
                    .defrag
                    .handle_end_of_stream()
                    .map(Params::decode)
                    .transpose()?;

                self.inner = State::Stdin;

                if params.is_none() {
                    return Err(ParseRequestError::ParamsMustBeLargerThanZero);
                }

                params.map(Part::from)
            }

            (State::Stdin, Transition::Parse(frame)) => {
                let (_, record_type, payload) = frame.into_parts();

                validate_record_type(record_type, Standard::Stdin)?;

                self.defrag.insert_payload(payload)?;

                None
            }
            (State::Stdin, Transition::EndOfStream(record_type)) => {
                validate_record_type(record_type, Standard::Stdin)?;

                let stdin = self
                    .defrag
                    .handle_end_of_stream()
                    .map(Stdin::decode)
                    .transpose()?;

                self.inner = match self
                    .role
                    .expect("Invalid state reached while parsing the request.")
                {
                    Role::Filter => State::Data,
                    _ => State::Finished,
                };

                Some(Part::from(stdin))
            }

            (State::Data, Transition::Parse(frame)) => {
                let (_, record_type, payload) = frame.into_parts();

                validate_record_type(record_type, Standard::Data)?;

                self.defrag.insert_payload(payload)?;

                None
            }
            (State::Data, Transition::EndOfStream(record_type)) => {
                validate_record_type(record_type, Standard::Data)?;

                let data = self
                    .defrag
                    .handle_end_of_stream()
                    .map(Data::decode)
                    .transpose()?;

                self.inner = State::Finished;

                if data.is_none() {
                    return Err(ParseRequestError::DataIsRequiredForFilterApplications);
                }

                data.map(Part::from)
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
