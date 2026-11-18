use termoxide_layout::measure::{AvailableSpace, Measure, MeasuredSize, Size, WidthOracle};

/// Two cells per character, like a terminal showing only wide glyphs.
struct Wide;

impl WidthOracle for Wide {
    fn width(&self, text: &str) -> u16 { u16::try_from(text.chars().count() * 2).unwrap_or(u16::MAX) }
}

/// A test leaf: one word per line at min-content, one line at max-content,
/// cut to a definite width.
struct Words(&'static str);

impl Measure for Words {
    fn measure(&self, available: Size<AvailableSpace>, oracle: &dyn WidthOracle) -> MeasuredSize {
        let words: Vec<&str> = self.0.split(' ').collect();
        let longest = words.iter().map(|w| oracle.width(w)).max().unwrap_or(0);
        let line = oracle.width(self.0);
        let line = match available.width {
            AvailableSpace::Definite(cells) => line.min(cells),
            AvailableSpace::MinContent | AvailableSpace::MaxContent => line,
        };
        MeasuredSize {
            min_content: Size { width: longest, height: u16::try_from(words.len()).unwrap_or(u16::MAX) },
            max_content: Size { width: line, height: 1 },
        }
    }
}

#[test]
fn a_leaf_measures_through_the_oracle_it_is_given() {
    let max = Size { width: AvailableSpace::MaxContent, height: AvailableSpace::MaxContent };
    assert_eq!(Words("ab cde").measure(max, &Wide), MeasuredSize {
        min_content: Size { width: 6, height: 2 },
        max_content: Size { width: 12, height: 1 },
    });
}

#[test]
fn definite_space_reaches_the_leaf() {
    let definite = Size { width: AvailableSpace::Definite(5), height: AvailableSpace::MinContent };
    assert_eq!(Words("ab cde").measure(definite, &Wide).max_content.width, 5);
}

#[test]
fn the_contract_is_object_safe() {
    let leaves: Vec<Box<dyn Measure>> = vec![Box::new(Words("a")), Box::new(Words("b c"))];
    let min = Size { width: AvailableSpace::MinContent, height: AvailableSpace::MinContent };
    let heights: Vec<u16> = leaves.iter().map(|l| l.measure(min, &Wide).min_content.height).collect();
    assert_eq!(heights, vec![1, 2]);
    assert_eq!(MeasuredSize::default().max_content, Size { width: 0, height: 0 });
}
