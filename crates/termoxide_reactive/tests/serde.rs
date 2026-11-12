//! Signals cross a byte boundary as their values.

use termoxide_reactive::{Signal, runtime::with_owner};

#[test]
fn a_signal_round_trips_as_its_value() {
    with_owner(|| {
        let count = Signal::new(vec![1u32, 2, 3]);
        count.update(|v| v.push(4));

        let bytes = postcard::to_allocvec(&count).unwrap();
        assert_eq!(bytes, postcard::to_allocvec(&vec![1u32, 2, 3, 4]).unwrap());

        let back: Signal<Vec<u32>> = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.get_untracked(), vec![1, 2, 3, 4]);
    });
}

#[test]
fn a_deserialized_signal_is_independent_of_the_original() {
    with_owner(|| {
        let original = Signal::new(String::from("before"));
        let copy: Signal<String> = postcard::from_bytes(&postcard::to_allocvec(&original).unwrap()).unwrap();

        original.set(String::from("after"));

        assert_eq!(copy.get_untracked(), "before");
    });
}
