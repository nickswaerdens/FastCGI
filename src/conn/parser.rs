use std::fmt::Debug;

use bytes::{BufMut, BytesMut};

use crate::{
    meta::{Meta, Stream},
    record::{DecodeFrame, DecodeFrameError, RecordType, RequestPart, ResponsePart},
};

use crate::codec::Frame;

type ParserResult<T> = Result<T, ParserError>;

#[derive(Debug)]
pub enum ParserError {
    InvalidState,
    UnexpectedRecordType(RecordType),
    UnexpectedEndOfStream,
    UnexpectedAbortRequest,
    DecodeFrameError(DecodeFrameError),
    StdIoError(std::io::Error),
}

impl From<std::io::Error> for ParserError {
    fn from(value: std::io::Error) -> Self {
        ParserError::StdIoError(value)
    }
}

impl From<DecodeFrameError> for ParserError {
    fn from(value: DecodeFrameError) -> Self {
        ParserError::DecodeFrameError(value)
    }
}

pub(crate) trait Parser {
    type State: Default + Debug;
    type Transition;
    type Output;

    fn parse_transition(frame: Frame) -> ParserResult<Self::Transition>;

    fn parse_frame(
        state: &mut Self::State,
        transition: Self::Transition,
    ) -> ParserResult<Option<Self::Output>>;
}

impl Parser for client::ResponseParser {
    type State = client::ConnectionState;
    type Transition = client::Transition;
    type Output = ResponsePart;

    fn parse_transition(frame: Frame) -> ParserResult<Self::Transition> {
        Self::Transition::parse(frame)
    }

    fn parse_frame(
        state: &mut Self::State,
        transition: Self::Transition,
    ) -> ParserResult<Option<Self::Output>> {
        Self::parse_frame(state, transition)
    }
}

impl Parser for server::RequestParser {
    type State = server::ConnectionState;
    type Transition = server::Transition;
    type Output = RequestPart;

    fn parse_transition(frame: Frame) -> ParserResult<Self::Transition> {
        Ok(Self::Transition::parse(frame))
    }

    fn parse_frame(
        state: &mut Self::State,
        transition: Self::Transition,
    ) -> ParserResult<Option<Self::Output>> {
        Self::parse_frame(state, transition)
    }
}

#[derive(Debug)]
pub enum ParserMode {
    Fragmented,
    Full(Full),
}

/// ParserMode which combines fragments into full frames before parsing.
///
/// This mode can be used for fragmented types which can only be validated in full. It
/// also makes it easier to work with parsed frames.
///
/// The cost is a couple reallocations to turn the payload of the fragmented frames into a
/// contiguous byte slice.
///
/// The default maximum size of the payload is 128MB (8 full frames). This can be adjusted
/// with `with_max_payload_size`. As the project is at an early stage, it's recommended to
/// manually set the maximum to avoid unexpected changes to the maximum payload size in the
/// future.
///
/// For parsing very large payloads, it's recommended to use the `Fragmented` mode instead.
#[derive(Debug)]
pub struct Full {
    frames: Vec<Frame>,
    max_total_payload: usize,
    current_total_payload: usize,
}

impl Full {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_payload_size(mut self, n: usize) -> Self {
        self.max_total_payload = n;
        self
    }

    fn insert_frame(&mut self, frame: Frame) {
        let n = frame.payload.len();

        if self.max_total_payload < self.current_total_payload + n {
            todo!();
        }

        // Make sure the frames belong to the same record.
        if !self.frames.is_empty() {
            let first = &self.frames[0];

            if first.get_id() != frame.get_id()
                || first.get_record_type() != frame.get_record_type()
            {
                todo!()
            }
        }

        self.frames.push(frame);
        self.current_total_payload += n;
    }

    fn merge_frames(&mut self) -> Option<Frame> {
        if self.frames.is_empty() {
            return None;
        }

        // Should this much space be reserved beforehand?
        // The frames drain iter could be chunked, with memory being reserved for each chunk instead.
        let mut buffer = BytesMut::with_capacity(self.current_total_payload);

        let header = self.frames[0].header;

        for frame in self.frames.drain(..) {
            buffer.put(frame.payload);
        }

        Some(Frame {
            header,
            payload: buffer,
        })
    }
}

impl Default for Full {
    fn default() -> Self {
        Self {
            frames: Vec::new(),
            max_total_payload: 0x8000000, // 128 MB
            current_total_payload: 0,
        }
    }
}

/// Handles the parsing of frame streams based on the parser mode.
fn handle_parse_frame<T: DecodeFrame, R>(
    frame: Frame,
    mode: &mut ParserMode,
) -> ParserResult<Option<R>>
where
    T: Into<R> + Meta<DataKind = Stream>,
{
    let record = match mode {
        ParserMode::Fragmented => Some(T::decode(frame.payload)?.into()),
        ParserMode::Full(full) => {
            full.insert_frame(frame);

            None
        }
    };

    Ok(record)
}

/// Handles the parsing of the end of a stream based on the parser mode.
fn handle_end_of_stream<T: DecodeFrame, R>(mode: &mut ParserMode) -> ParserResult<Option<R>>
where
    T: Into<R> + Meta<DataKind = Stream>,
{
    let record = match mode {
        ParserMode::Fragmented => None,
        ParserMode::Full(full) => {
            let frame = full.merge_frames();

            match frame {
                Some(frame) => Some(T::decode(frame.payload)?.into()),
                _ => None,
            }
        }
    };

    Ok(record)
}

pub mod client {
    use crate::{
        codec::Frame,
        record::{
            DecodeFrame, DecodeFrameError, EndRequest, RecordType, ResponsePart, Standard, Stderr,
            Stdout,
        },
    };

    use super::{
        handle_end_of_stream, handle_parse_frame, Full, ParserError, ParserMode, ParserResult,
    };

    #[derive(Debug, Clone, Copy)]
    pub(crate) enum StreamState {
        Init,
        Started,
        Ended,
    }

    #[derive(Debug, Clone, Copy)]
    pub(crate) enum State {
        Std { out: StreamState, err: StreamState },
        Finished,
    }

    #[derive(Debug)]
    pub(crate) enum Transition {
        ParseStdout(Frame),
        ParseStderr(Frame),
        EndOfStdout,
        EndOfStderr,
        ParseEndRequest(Frame),
    }

    impl Transition {
        pub(crate) fn parse(frame: Frame) -> ParserResult<Transition> {
            if frame.get_id() == 0 {
                unimplemented!()
            }

            let transition = match (frame.get_record_type(), frame.payload.is_empty()) {
                (RecordType::Standard(Standard::Stdout), false) => Transition::ParseStdout(frame),
                (RecordType::Standard(Standard::Stdout), true) => Transition::EndOfStdout,

                (RecordType::Standard(Standard::Stderr), false) => Transition::ParseStderr(frame),
                (RecordType::Standard(Standard::Stderr), true) => Transition::EndOfStderr,

                (RecordType::Standard(Standard::EndRequest), false) => {
                    Transition::ParseEndRequest(frame)
                }
                (RecordType::Standard(Standard::EndRequest), true) => {
                    return Err(ParserError::DecodeFrameError(
                        DecodeFrameError::InsufficientDataInBuffer,
                    ))
                }

                (record_type, _) => return Err(ParserError::UnexpectedRecordType(record_type)),
            };

            Ok(transition)
        }
    }

    #[derive(Debug)]
    pub(crate) struct ConnectionState {
        inner: State,

        // stdout and stderr can be interleaved.
        stdout_mode: ParserMode,
        stderr_mode: ParserMode,
    }

    impl ConnectionState {
        pub(crate) fn new() -> Self {
            Self::default()
        }

        pub fn stdout_mode(&self) -> &ParserMode {
            &self.stdout_mode
        }

        pub fn stderr_mode(&self) -> &ParserMode {
            &self.stderr_mode
        }
    }

    impl Default for ConnectionState {
        fn default() -> Self {
            Self {
                inner: State::Std {
                    out: StreamState::Init,
                    err: StreamState::Init,
                },
                // Fragmented is not yet supported by the Client and Server.
                stdout_mode: ParserMode::Full(Full::default()),
                stderr_mode: ParserMode::Full(Full::default()),
            }
        }
    }

    #[derive(Debug)]
    pub(crate) struct ResponseParser;

    impl ResponseParser {
        pub(crate) fn parse_frame(
            state: &mut ConnectionState,
            transition: Transition,
        ) -> ParserResult<Option<ResponsePart>> {
            match (state.inner, transition) {
                // Stdout
                (
                    State::Std {
                        out: StreamState::Init,
                        err,
                    },
                    Transition::ParseStdout(frame),
                ) => {
                    let record = handle_parse_frame::<Stdout, _>(frame, &mut state.stdout_mode);

                    state.inner = State::Std {
                        out: StreamState::Started,
                        err,
                    };

                    record
                }
                (
                    State::Std {
                        out: StreamState::Started,
                        ..
                    },
                    Transition::ParseStdout(frame),
                ) => handle_parse_frame::<Stdout, _>(frame, &mut state.stdout_mode),
                (
                    State::Std {
                        out: StreamState::Started,
                        err,
                    },
                    Transition::EndOfStdout,
                ) => {
                    let record = handle_end_of_stream::<Stdout, _>(&mut state.stdout_mode);

                    state.inner = State::Std {
                        out: StreamState::Ended,
                        err,
                    };

                    record
                }

                // Stderr
                (
                    State::Std {
                        err: StreamState::Init,
                        out,
                    },
                    Transition::ParseStderr(frame),
                ) => {
                    let record = handle_parse_frame::<Stderr, _>(frame, &mut state.stderr_mode);

                    state.inner = State::Std {
                        err: StreamState::Started,
                        out,
                    };

                    record
                }
                (
                    State::Std {
                        err: StreamState::Started,
                        ..
                    },
                    Transition::ParseStderr(frame),
                ) => handle_parse_frame::<Stderr, _>(frame, &mut state.stderr_mode),

                // Optionally parse empty stderr requests, even if there was no actual stderr response.
                (
                    State::Std {
                        err: StreamState::Started | StreamState::Init,
                        out,
                    },
                    Transition::EndOfStderr,
                ) => {
                    let record = handle_end_of_stream::<Stderr, _>(&mut state.stderr_mode);

                    state.inner = State::Std {
                        err: StreamState::Ended,
                        out,
                    };

                    record
                }

                // EndRequest
                (
                    State::Std {
                        out: StreamState::Ended,
                        err: StreamState::Init | StreamState::Ended,
                    },
                    Transition::ParseEndRequest(frame),
                ) => {
                    let end_request = EndRequest::decode(frame.payload)?;

                    state.inner = State::Finished;

                    Ok(Some(end_request.into()))
                }

                // Invalid state
                _ => Err(ParserError::InvalidState),
            }
        }
    }
}

pub mod server {
    use crate::{
        codec::Frame,
        record::{
            begin_request::Role, AbortRequest, BeginRequest, Data, DecodeFrame, Params, RecordType,
            RequestPart, Standard, Stdin,
        },
    };

    use super::{
        handle_end_of_stream, handle_parse_frame, Full, ParserError, ParserMode, ParserResult,
    };

    pub(crate) fn validate_record_type(
        lh: RecordType,
        rh: impl PartialEq<RecordType>,
    ) -> ParserResult<()> {
        (rh == lh)
            .then_some(())
            .ok_or(ParserError::UnexpectedRecordType(lh))
    }

    #[derive(Debug, Clone, Copy)]
    pub(crate) enum State {
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
            if frame.get_id() == 0 {
                unimplemented!()
            }

            if !frame.payload.is_empty() {
                Transition::Parse(frame)
            } else if frame.get_record_type() == Standard::AbortRequest {
                Transition::Abort
            } else {
                Transition::EndOfStream(frame.get_record_type())
            }
        }
    }

    #[derive(Debug)]
    pub(crate) struct ConnectionState {
        inner: State,
        role: Option<Role>,
        mode: ParserMode,
    }

    impl ConnectionState {
        pub(crate) fn new() -> Self {
            Self::default()
        }

        pub(crate) fn mode(&self) -> &ParserMode {
            &self.mode
        }
    }

    impl Default for ConnectionState {
        fn default() -> Self {
            Self {
                inner: State::BeginRequest,
                role: None,
                mode: ParserMode::Full(Full::default()),
                // Fragmented is not yet supported by the Client and Server.
                // mode: ParserMode::Fragmented,
            }
        }
    }

    #[derive(Debug)]
    pub(crate) struct RequestParser;

    impl RequestParser {
        pub(crate) fn parse_frame(
            state: &mut ConnectionState,
            transition: Transition,
        ) -> ParserResult<Option<RequestPart>> {
            match (state.inner, transition) {
                (State::BeginRequest, Transition::Parse(frame)) => {
                    let (header, payload) = frame.into_parts();

                    validate_record_type(header.record_type, Standard::BeginRequest)?;

                    let begin_request = BeginRequest::decode(payload)?;

                    state.role = Some(begin_request.get_role());
                    state.inner = State::Params;

                    Ok(Some(begin_request.into()))
                }

                (State::Params, Transition::Parse(frame)) => {
                    validate_record_type(frame.get_record_type(), Standard::Params)?;

                    handle_parse_frame::<Params, _>(frame, &mut state.mode)
                }
                (State::Params, Transition::EndOfStream(record_type)) => {
                    validate_record_type(record_type, Standard::Params)?;

                    let record = handle_end_of_stream::<Params, _>(&mut state.mode)?;

                    state.inner = State::Stdin;

                    Ok(record)
                }

                (State::Stdin, Transition::Parse(frame)) => {
                    validate_record_type(frame.get_record_type(), Standard::Stdin)?;

                    handle_parse_frame::<Stdin, _>(frame, &mut state.mode)
                }
                (State::Stdin, Transition::EndOfStream(record_type)) => {
                    validate_record_type(record_type, Standard::Stdin)?;

                    let record = handle_end_of_stream::<Stdin, _>(&mut state.mode)?;

                    state.inner = match state.role.ok_or(ParserError::InvalidState)? {
                        Role::Filter => State::Data,
                        _ => State::Finished,
                    };

                    Ok(record)
                }

                (State::Data, Transition::Parse(frame)) => {
                    validate_record_type(frame.get_record_type(), Standard::Data)?;

                    handle_parse_frame::<Data, _>(frame, &mut state.mode)
                }
                (State::Data, Transition::EndOfStream(record_type)) => {
                    validate_record_type(record_type, Standard::Data)?;

                    let record = handle_end_of_stream::<Data, _>(&mut state.mode)?;

                    state.inner = State::Finished;

                    Ok(record)
                }

                // Abort
                (State::Params | State::Stdin | State::Data, Transition::Abort) => {
                    state.inner = State::Aborted;

                    Ok(Some(AbortRequest.into()))
                }

                // Errors
                (_, Transition::EndOfStream(_)) => Err(ParserError::UnexpectedEndOfStream),
                (_, Transition::Abort) => Err(ParserError::UnexpectedAbortRequest),
                _ => Err(ParserError::InvalidState),
            }
        }
    }
}
