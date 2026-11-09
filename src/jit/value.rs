pub const TAG_NULL: u64 = 0;
pub const TAG_INT: u64 = 1;
pub const TAG_FLOAT: u64 = 2;
pub const TAG_BOOL: u64 = 3;
pub const TAG_HEAP: u64 = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LlvmValue {
    pub tag: u64,
    pub payload: u64,
}

impl LlvmValue {
    pub const NULL: Self = Self {
        tag: TAG_NULL,
        payload: 0,
    };

    #[inline(always)]
    pub fn int(val: i64) -> Self {
        Self {
            tag: TAG_INT,
            payload: val as u64,
        }
    }

    #[inline(always)]
    pub fn float(val: f64) -> Self {
        Self {
            tag: TAG_FLOAT,
            payload: val.to_bits(),
        }
    }

    #[inline(always)]
    pub fn bool(val: bool) -> Self {
        Self {
            tag: TAG_BOOL,
            payload: if val { 1 } else { 0 },
        }
    }

    #[inline(always)]
    pub fn heap(idx: usize) -> Self {
        Self {
            tag: TAG_HEAP,
            payload: idx as u64,
        }
    }

    #[inline(always)]
    pub fn is_int(&self) -> bool {
        self.tag == TAG_INT
    }

    #[inline(always)]
    pub fn as_int(&self) -> i64 {
        self.payload as i64
    }

    #[inline(always)]
    pub fn is_float(&self) -> bool {
        self.tag == TAG_FLOAT
    }

    #[inline(always)]
    pub fn as_float(&self) -> f64 {
        f64::from_bits(self.payload)
    }

    #[inline(always)]
    pub fn is_bool(&self) -> bool {
        self.tag == TAG_BOOL
    }

    #[inline(always)]
    pub fn as_bool(&self) -> bool {
        self.payload != 0
    }

    #[inline(always)]
    pub fn is_null(&self) -> bool {
        self.tag == TAG_NULL
    }

    #[inline(always)]
    pub fn is_truthy(&self) -> bool {
        match self.tag {
            TAG_BOOL => self.payload != 0,
            TAG_NULL => false,
            TAG_INT => (self.payload as i64) != 0,
            _ => true,
        }
    }
}
