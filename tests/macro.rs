use fastcgi::{
    build_enum_with_from_impls,
    record::{EndRequest, Stderr, Stdout},
};

/*

    cargo expand --test macro

*/

build_enum_with_from_impls! {
    pub(crate) Part {
        EndRequest(EndRequest),
        Stdout(Option<Stdout>),
        Stderr(Option<Stderr>),
    }
}
