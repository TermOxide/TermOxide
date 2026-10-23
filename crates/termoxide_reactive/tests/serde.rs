//! Signals cross a byte boundary as their values.

use termoxide_reactive::{Signal, Unrecorded, recorded_only, runtime::with_owner};

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

#[test]
fn an_unrecorded_signal_round_trips_as_its_value_too() {
    with_owner(|| {
        let ticks = Signal::new(7u64).unrecorded();

        let back: Signal<u64, Unrecorded> = postcard::from_bytes(&postcard::to_allocvec(&ticks).unwrap()).unwrap();

        assert_eq!(back.get_untracked(), 7);
    });
}

#[test]
fn a_signal_switches_mode_without_losing_its_value() {
    with_owner(|| {
        let ticks = Signal::new(1u64).unrecorded();
        ticks.set(2);

        assert_eq!(recorded_only(|| postcard::to_allocvec(&ticks).unwrap()), [0u8; 0]);
        assert_eq!(ticks.recorded().get_untracked(), 2);
    });
}

#[test]
fn only_recorded_signals_count_in_a_recorded_only_encoding() {
    with_owner(|| {
        let (count, ticks) = (Signal::new(1u32), Signal::new(1u64).unrecorded());
        let encode = || recorded_only(|| postcard::to_allocvec(&(count, ticks)).unwrap());
        let before = encode();

        ticks.set(2);
        assert_eq!(encode(), before);

        count.set(2);
        assert_ne!(encode(), before);
        assert_eq!(
            postcard::to_allocvec(&(count, ticks)).unwrap(),
            postcard::to_allocvec(&(2u32, 2u64)).unwrap()
        );
    });
}
