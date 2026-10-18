use std::collections::{HashMap, HashSet};

use crate::bytecode::*;

struct Instruction {
    start: usize,
    end: usize,
    relocations: Vec<(usize, usize)>,
}

pub fn optimize(mut bytecode: Bytecode) -> Bytecode {
    let Some(instructions) = decode(&bytecode.code) else {
        return bytecode;
    };
    let target_offsets: HashSet<usize> = instructions
        .iter()
        .flat_map(|instruction| instruction.relocations.iter().map(|(_, target)| *target))
        .collect();

    let mut removed = vec![false; instructions.len()];
    let mut index = 0;
    while index + 1 < instructions.len() {
        let first = &instructions[index];
        let second = &instructions[index + 1];
        if !target_offsets.contains(&second.start)
            && is_redundant_pair(&bytecode.code, first, second)
        {
            removed[index] = true;
            removed[index + 1] = true;
            index += 2;
        } else {
            index += 1;
        }
    }

    if !removed.iter().any(|remove| *remove) {
        return bytecode;
    }

    let mut offsets = HashMap::with_capacity(instructions.len() + 1);
    let mut optimized = Vec::with_capacity(bytecode.code.len());
    for (index, instruction) in instructions.iter().enumerate() {
        offsets.insert(instruction.start, optimized.len());
        if !removed[index] {
            optimized.extend_from_slice(&bytecode.code[instruction.start..instruction.end]);
        }
    }
    offsets.insert(bytecode.code.len(), optimized.len());

    for (index, instruction) in instructions.iter().enumerate() {
        if removed[index] {
            continue;
        }
        let new_start = offsets[&instruction.start];
        for &(operand_offset, old_target) in &instruction.relocations {
            let new_target = offsets[&old_target] as u32;
            let new_operand = new_start + operand_offset - instruction.start;
            optimized[new_operand..new_operand + 4].copy_from_slice(&new_target.to_le_bytes());
        }
    }
    bytecode.code = optimized;
    bytecode
}

fn is_redundant_pair(code: &[u8], first: &Instruction, second: &Instruction) -> bool {
    let first_opcode = code[first.start];
    let second_opcode = code[second.start];

    if second_opcode == OP_POP
        && matches!(
            first_opcode,
            OP_CONSTANT | OP_NULL | OP_LOAD_GLOBAL | OP_LOAD_LOCAL | OP_LOAD_UPVALUE
        )
    {
        return true;
    }

    let store_opcode = match first_opcode {
        OP_LOAD_GLOBAL => OP_STORE_GLOBAL,
        OP_LOAD_LOCAL => OP_STORE_LOCAL,
        OP_LOAD_UPVALUE => OP_STORE_UPVALUE,
        _ => return false,
    };
    second_opcode == store_opcode
        && Bytecode::read_u32(code, first.start + 1) == Bytecode::read_u32(code, second.start + 1)
}

fn decode(code: &[u8]) -> Option<Vec<Instruction>> {
    let mut instructions = Vec::new();
    let mut ip = 0;
    while ip < code.len() {
        let opcode = *code.get(ip)?;
        let mut relocations = Vec::new();
        let end = match opcode {
            OP_CONSTANT | OP_LOAD_GLOBAL | OP_STORE_GLOBAL | OP_INC_GLOBAL | OP_LOAD_LOCAL
            | OP_STORE_LOCAL | OP_LOAD_UPVALUE | OP_STORE_UPVALUE | OP_INC_LOCAL => {
                ip.checked_add(5)?
            }
            OP_JUMP | OP_JUMP_IF_FALSE => {
                let target_offset = ip.checked_add(1)?;
                let target = read_u32(code, target_offset)? as usize;
                relocations.push((target_offset, target));
                ip.checked_add(5)?
            }
            OP_ADD_GLOBAL | OP_ADD_LOCAL => ip.checked_add(13)?,
            OP_APPEND_STRING_GLOBAL => ip.checked_add(9)?,
            OP_LOOP_INC_LESS | OP_LOOP_INC_LESS_LOCAL => ip.checked_add(13)?,
            OP_LOOP_STEP_LESS | OP_LOOP_STEP_LESS_LOCAL => ip.checked_add(21)?,
            OP_LESS_CONST_JUMP_IF_FALSE => {
                let target_offset = ip.checked_add(13)?;
                let target = read_u32(code, target_offset)? as usize;
                relocations.push((target_offset, target));
                ip.checked_add(17)?
            }
            OP_MAKE_CLOSURE => {
                let count = read_u32(code, ip.checked_add(9)?)? as usize;
                let target = read_u32(code, ip.checked_add(1)?)? as usize;
                relocations.push((ip + 1, target));
                ip.checked_add(13)?.checked_add(count.checked_mul(5)?)?
            }
            OP_CALL_FUNCTION => ip.checked_add(5)?,
            OP_INPUT => ip.checked_add(2)?,
            OP_BUILD_ARRAY => ip.checked_add(6)?,
            OP_BUILD_OBJECT => {
                let count = read_u32(code, ip.checked_add(1)?)? as usize;
                ip.checked_add(5)?.checked_add(count.checked_mul(4)?)?
            }
            OP_OBJECT_GET_CONST | OP_OBJECT_GET_OR_CREATE_CONST => ip.checked_add(5)?,
            OP_FOR_IN_ARRAY_HEADER => {
                let target_offset = ip.checked_add(9)?;
                let target = read_u32(code, target_offset)? as usize;
                relocations.push((target_offset, target));
                ip.checked_add(13)?
            }
            OP_FOR_IN_ARRAY_NEXT => {
                let target_offset = ip.checked_add(5)?;
                let target = read_u32(code, target_offset)? as usize;
                relocations.push((target_offset, target));
                ip.checked_add(9)?
            }
            OP_ARRAY_PUSH | OP_WAIT => ip.checked_add(2)?,
            OP_ADD
            | OP_SUBTRACT
            | OP_MULTIPLY
            | OP_DIVIDE
            | OP_EQUALS
            | OP_NOT_EQUALS
            | OP_GREATER
            | OP_LESS
            | OP_GREATER_EQUAL
            | OP_LESS_EQUAL
            | OP_AND
            | OP_OR
            | OP_NOT
            | OP_PRINT
            | OP_POP
            | OP_NULL
            | OP_RETURN
            | OP_INDEX_GET
            | OP_INDEX_SET
            | OP_ARRAY_LEN
            | OP_ARRAY_POP
            | OP_OBJECT_GET
            | OP_OBJECT_SET
            | OP_OBJECT_DELETE
            | OP_OBJECT_KEYS
            | OP_OBJECT_GET_OR_CREATE
            | OP_IS_ARRAY
            | OP_IS_OBJECT
            | OP_MEMBER_LENGTH
            | OP_TYPE => ip.checked_add(1)?,
            _ => return None,
        };
        if end > code.len() || relocations.iter().any(|(_, target)| *target > code.len()) {
            return None;
        }
        instructions.push(Instruction {
            start: ip,
            end,
            relocations,
        });
        ip = end;
    }

    let boundaries: HashSet<usize> = instructions
        .iter()
        .map(|instruction| instruction.start)
        .chain(std::iter::once(code.len()))
        .collect();
    if instructions
        .iter()
        .flat_map(|instruction| instruction.relocations.iter())
        .any(|(_, target)| !boundaries.contains(target))
    {
        return None;
    }
    Some(instructions)
}

fn read_u32(code: &[u8], offset: usize) -> Option<u32> {
    let bytes = code.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_discarded_values_and_relocates_jumps() {
        let mut bytecode = Bytecode::new();
        bytecode.emit_byte(OP_JUMP);
        let target = bytecode.emit_u32(0);
        bytecode.emit_byte(OP_CONSTANT);
        bytecode.emit_u32(0);
        bytecode.emit_byte(OP_POP);
        let destination = bytecode.code.len();
        bytecode.patch_u32(target, destination as u32);
        bytecode.emit_byte(OP_NULL);

        let optimized = optimize(bytecode);
        assert_eq!(optimized.code, vec![OP_JUMP, 5, 0, 0, 0, OP_NULL]);
    }

    #[test]
    fn retains_a_pop_that_is_a_jump_target() {
        let mut bytecode = Bytecode::new();
        bytecode.emit_byte(OP_JUMP);
        let target = bytecode.emit_u32(0);
        bytecode.emit_byte(OP_CONSTANT);
        bytecode.emit_u32(0);
        let pop_offset = bytecode.emit_byte(OP_POP);
        bytecode.patch_u32(target, pop_offset as u32);

        let optimized = optimize(bytecode);
        assert_eq!(optimized.code[pop_offset], OP_POP);
    }
}
