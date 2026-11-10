pub const STATES: [(&str, &str); 5] = [
    ("unknown", "待判断"),
    ("in_progress", "进行中"),
    ("review", "待审阅"),
    ("blocked", "已阻塞"),
    ("to_archive", "待归档"),
];

pub const KEYS: [&str; 7] = [
    "unknown", "in_progress", "review", "blocked", "to_archive", "completed", "alien",
];

#[derive(Clone, Copy, Debug)]
pub struct Configuration(pub u8);

impl Configuration {
    pub fn valid(self) -> bool {
        self.0 == 31
    }

    pub fn edit(self, key: usize) -> Self {
        Self(self.0 ^ (1 << key))
    }

    pub fn migrated(self) -> Self {
        Self(self.0 & !(1 << 5))
    }
}

pub fn account(memberships: [u8; 2]) -> Option<usize> {
    match memberships {
        [31, second] if second != 31 => Some(0),
        [first, 31] if first != 31 => Some(1),
        _ => None,
    }
}

pub fn permutations() -> Vec<[usize; 5]> {
    (0..3125).filter_map(|mut number| {
        let mut order = [0; 5];
        for index in &mut order {
            *index = number % 5;
            number /= 5;
        }
        let mut sorted = order;
        sorted.sort();
        (sorted == [0, 1, 2, 3, 4]).then_some(order)
    }).collect()
}
