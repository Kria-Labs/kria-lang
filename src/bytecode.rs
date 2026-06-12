use crate::vm::Value;

pub const OP_CONSTANT: u8 = 0;
pub const OP_LOAD_GLOBAL: u8 = 1;
pub const OP_STORE_GLOBAL: u8 = 2;
pub const OP_ADD: u8 = 3;
pub const OP_SUBTRACT: u8 = 4;
pub const OP_MULTIPLY: u8 = 5;
pub const OP_DIVIDE: u8 = 6;
pub const OP_EQUALS: u8 = 7;
pub const OP_NOT_EQUALS: u8 = 8;
pub const OP_GREATER: u8 = 9;
pub const OP_LESS: u8 = 10;
pub const OP_GREATER_EQUAL: u8 = 11;
pub const OP_LESS_EQUAL: u8 = 12;
pub const OP_AND: u8 = 13;
pub const OP_OR: u8 = 14;
pub const OP_NOT: u8 = 15;
pub const OP_PRINT: u8 = 16;
pub const OP_POP: u8 = 17;
pub const OP_JUMP: u8 = 18;
pub const OP_JUMP_IF_FALSE: u8 = 19;
pub const OP_INC_GLOBAL: u8 = 20;
pub const OP_ADD_GLOBAL: u8 = 21;
// Combined instructions for loop hot path
pub const OP_LESS_CONST_JUMP_IF_FALSE: u8 = 22; // LoadGlobal + Constant + LessThan + JumpIfFalse
pub const OP_LOOP_INC_LESS: u8 = 23;            // IncGlobal + Jump (full loop tick)
pub const OP_NULL: u8 = 24;
pub const OP_MAKE_CLOSURE: u8 = 25;             // offset, num_params, num_upvalues, [kind, index]*
pub const OP_CALL_FUNCTION: u8 = 26;            // Call function: num_args
pub const OP_RETURN: u8 = 27;                   // Return from function
pub const OP_LOAD_LOCAL: u8 = 28;               // Load local variable: frame_offset
pub const OP_STORE_LOCAL: u8 = 29;              // Store local variable: frame_offset
pub const OP_INPUT: u8 = 30;                    // Input with type mask: type_mask (bit 0: str, bit 1: int, bit 2: float)
pub const OP_LOAD_UPVALUE: u8 = 31;             // Load captured variable: upvalue_index
pub const OP_STORE_UPVALUE: u8 = 32;            // Store captured variable: upvalue_index
pub const OP_BUILD_ARRAY: u8 = 33;             // u32 count, u8 mutable
pub const OP_INDEX_GET: u8 = 34;
pub const OP_INDEX_SET: u8 = 35;
pub const OP_ARRAY_LEN: u8 = 36;
pub const OP_ARRAY_PUSH: u8 = 37;
pub const OP_ARRAY_POP: u8 = 38;
pub const OP_BUILD_OBJECT: u8 = 39;
pub const OP_OBJECT_GET_CONST: u8 = 40;
pub const OP_OBJECT_GET: u8 = 41;
pub const OP_OBJECT_SET: u8 = 42;
pub const OP_OBJECT_DELETE: u8 = 43;
pub const OP_OBJECT_KEYS: u8 = 44;
pub const OP_OBJECT_GET_OR_CREATE_CONST: u8 = 45;
pub const OP_OBJECT_GET_OR_CREATE: u8 = 46;
pub const OP_IS_ARRAY: u8 = 47;
pub const OP_IS_OBJECT: u8 = 48;
pub const OP_MEMBER_LENGTH: u8 = 49;
pub const OP_FOR_IN_ARRAY_HEADER: u8 = 50; // arr_g, i_g, exit; pushes item or jumps
pub const OP_FOR_IN_ARRAY_NEXT: u8 = 51;  // i_g, loop_start
pub const OP_LOOP_STEP_LESS: u8 = 52;      // idx, limit, step — fused counter loop
pub const OP_TYPE: u8 = 53;                // Pop value, push type name string
pub const OP_WAIT: u8 = 54;                // Pop ms (number), sleep

pub const CAPTURE_LOCAL: u8 = 0;
pub const CAPTURE_UPVALUE: u8 = 1;

pub struct Bytecode {
    pub code: Vec<u8>,
    pub constants: Vec<Value>,
    pub num_globals: usize,
}

impl Bytecode {
    pub fn new() -> Self {
        Bytecode {
            code: Vec::new(),
            constants: Vec::new(),
            num_globals: 0,
        }
    }

    pub fn emit_byte(&mut self, byte: u8) -> usize {
        let pos = self.code.len();
        self.code.push(byte);
        pos
    }

    pub fn emit_u32(&mut self, value: u32) -> usize {
        let pos = self.code.len();
        self.code.extend_from_slice(&value.to_le_bytes());
        pos
    }

    pub fn emit_i64(&mut self, value: i64) -> usize {
        let pos = self.code.len();
        self.code.extend_from_slice(&value.to_le_bytes());
        pos
    }

    #[inline(always)]
    pub fn read_u32(code: &[u8], ip: usize) -> u32 {
        u32::from_le_bytes([code[ip], code[ip+1], code[ip+2], code[ip+3]])
    }

    #[inline(always)]
    pub unsafe fn read_u32_unchecked(code: &[u8], ip: usize) -> u32 {
        Self::read_u32(code, ip)
    }

    #[inline(always)]
    pub unsafe fn read_i64_unchecked(code: &[u8], ip: usize) -> i64 {
        Self::read_i64(code, ip)
    }

    #[inline(always)]
    pub fn read_i64(code: &[u8], ip: usize) -> i64 {
        i64::from_le_bytes([
            code[ip], code[ip+1], code[ip+2], code[ip+3],
            code[ip+4], code[ip+5], code[ip+6], code[ip+7],
        ])
    }

    pub fn patch_u32(&mut self, pos: usize, value: u32) {
        let bytes = value.to_le_bytes();
        self.code[pos] = bytes[0];
        self.code[pos+1] = bytes[1];
        self.code[pos+2] = bytes[2];
        self.code[pos+3] = bytes[3];
    }

    pub fn add_constant(&mut self, value: Value) -> u32 {
        for (i, c) in self.constants.iter().enumerate() {
            if *c == value { return i as u32; }
        }
        let idx = self.constants.len() as u32;
        self.constants.push(value);
        idx
    }
}
