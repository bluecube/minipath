use std::fmt::Display;

#[derive(Clone, PartialEq)]
pub struct Stats {
    pub count: usize,
    pub min: usize,
    pub max: usize,
    pub avg: f32,
}

impl Stats {
    pub fn new_single(v: usize) -> Self {
        Stats {
            count: 1,
            min: v,
            max: v,
            avg: v as f32,
        }
    }

    pub fn add_sample(&mut self, value: usize) {
        self.count += 1;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.avg += (value as f32 - self.avg) / (self.count as f32);
    }

    pub fn merge(&self, other: &Self) -> Self {
        Stats {
            count: self.count + other.count,
            min: self.min.min(other.min),
            max: self.max.max(other.max),
            avg: if self.count > 0 || other.count > 0 {
                (self.avg * self.count as f32 + other.avg * other.count as f32)
                    / (self.count + other.count) as f32
            } else {
                0.0
            },
        }
    }
}

impl Default for Stats {
    fn default() -> Self {
        Stats {
            count: 0,
            min: usize::MAX,
            max: 0,
            avg: 0.0,
        }
    }
}

impl Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - {}; avg {:.1}; {} samples",
            self.min, self.max, self.avg, self.count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::assert;

    #[test]
    fn new_single() {
        let s = Stats::new_single(10);
        assert!(s.count == 1);
        assert!(s.min == 10);
        assert!(s.max == 10);
        assert!(s.avg == 10.0);
    }

    #[test]
    fn add_sample() {
        let mut s = Stats::default();
        s.add_sample(20);
        assert!(s.count == 1);
        assert!(s.min == 20);
        assert!(s.max == 20);
        assert!(s.avg == 20.0);
    }

    #[test]
    fn merge_stats() {
        let a = Stats::new_single(10);
        let mut b = Stats::new_single(30);
        b.add_sample(50);
        let m = a.merge(&b);
        assert!(m.count == 3);
        assert!(m.min == 10);
        assert!(m.max == 50);
        assert!(m.avg == 30.0);
    }

    #[test]
    fn merge_with_default() {
        let default = Stats::default();
        let s = Stats::new_single(5);
        let merged = default.merge(&s);

        assert!(merged == s);
    }

    #[test]
    fn merge_two_default() {
        let default = Stats::default();
        let merged = default.merge(&default);

        assert!(merged == default);
    }

    #[test]
    fn display_format() {
        let s = Stats::new_single(42);
        let output = format!("{}", s);
        assert!(output.contains("42 - 42"));
        assert!(output.contains("avg 42.0"));
        assert!(output.contains("1 samples"));
    }
}
