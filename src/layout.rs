use crate::hir::HirType;
use crate::lir::TargetDataLayout;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AbiClass {
    Void,
    Integer,
    Float(u16),
    SlicePair,
    IndirectAggregate { slots: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TypeLayout {
    pub(crate) size_bytes: u64,
    pub(crate) align_bytes: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct TypeLayouts {
    target: TargetDataLayout,
    structs: HashMap<String, Vec<(String, HirType)>>,
    enums: HashMap<String, Vec<Option<HirType>>>,
}

impl TypeLayouts {
    pub(crate) fn new(
        target: TargetDataLayout,
        structs: HashMap<String, Vec<(String, HirType)>>,
        enums: HashMap<String, Vec<Option<HirType>>>,
    ) -> Self {
        Self {
            target,
            structs,
            enums,
        }
    }

    pub(crate) fn layout(&self, data_type: &HirType) -> Option<TypeLayout> {
        if self.target.pointer_bytes() == 0 || self.target.stack_slot_bytes() == 0 {
            return None;
        }
        self.layout_inner(data_type, &mut HashSet::new())
    }

    pub(crate) fn abi_class(&self, data_type: &HirType) -> Option<AbiClass> {
        Some(match data_type {
            HirType::Void => AbiClass::Void,
            HirType::Float { bits } => AbiClass::Float(*bits),
            HirType::Slice(_) => AbiClass::SlicePair,
            ty if self.is_aggregate_type(ty) => AbiClass::IndirectAggregate {
                slots: self.aggregate_slot_count(data_type)?,
            },
            _ => AbiClass::Integer,
        })
    }

    pub(crate) fn is_indirect_aggregate(&self, data_type: &HirType) -> bool {
        self.abi_class(data_type)
            .is_some_and(|class| matches!(class, AbiClass::IndirectAggregate { .. }))
    }

    pub(crate) fn aggregate_slot_count(&self, data_type: &HirType) -> Option<u32> {
        if !self.is_aggregate_type(data_type) {
            return None;
        }
        let size = self.layout(data_type)?.size_bytes;
        self.slots_for_bytes(size).map(|slots| slots.max(1))
    }

    pub(crate) fn slots_for_value(&self, data_type: &HirType) -> Option<u32> {
        let size = self.layout(data_type)?.size_bytes;
        self.slots_for_bytes(size).map(|slots| slots.max(1))
    }

    pub(crate) fn storage_stride(&self, data_type: &HirType) -> Option<u64> {
        Some(self.layout(data_type)?.size_bytes)
    }

    pub(crate) fn struct_field_offset_slots(&self, name: &str, member: &str) -> Option<u32> {
        let fields = self.structs.get(name)?;
        let slot_bytes = self.target.stack_slot_bytes();
        if slot_bytes == 0 {
            return None;
        }
        let mut offset = 0u64;
        for (field_name, field_type) in fields {
            if field_name == member {
                return u32::try_from(offset / slot_bytes).ok();
            }
            let field_size = self.layout(field_type)?.size_bytes;
            offset = offset.checked_add(self.round_to_slot(field_size)?)?;
        }
        None
    }

    pub(crate) fn enum_uses_storage(&self, name: &str) -> bool {
        self.enums
            .get(name)
            .is_some_and(|variants| variants.iter().any(Option::is_some))
    }

    pub(crate) fn enum_storage_slot_count(&self, name: &str) -> Option<u32> {
        if !self.enum_uses_storage(name) {
            return None;
        }
        self.aggregate_slot_count(&HirType::Named(name.to_string()))
    }

    fn is_aggregate_type(&self, data_type: &HirType) -> bool {
        matches!(data_type, HirType::Array { .. })
            || matches!(data_type, HirType::Named(name) if self.structs.contains_key(name) || self.enum_uses_storage(name))
    }

    fn slots_for_bytes(&self, bytes: u64) -> Option<u32> {
        let slot_bytes = self.target.stack_slot_bytes();
        if slot_bytes == 0 {
            return None;
        }
        u32::try_from(bytes.div_ceil(slot_bytes)).ok()
    }

    fn round_to_slot(&self, bytes: u64) -> Option<u64> {
        let slot_bytes = self.target.stack_slot_bytes();
        if slot_bytes == 0 {
            return None;
        }
        bytes.div_ceil(slot_bytes).checked_mul(slot_bytes)
    }

    fn layout_inner(
        &self,
        data_type: &HirType,
        visiting: &mut HashSet<String>,
    ) -> Option<TypeLayout> {
        if let Some(size) = scalar_storage_size(data_type) {
            return Some(TypeLayout {
                size_bytes: u64::from(size),
                align_bytes: u64::from(size).min(self.target.stack_slot_bytes()).max(1),
            });
        }
        match data_type {
            HirType::Void => Some(TypeLayout {
                size_bytes: 0,
                align_bytes: 1,
            }),
            HirType::Pointer(_) | HirType::Function { .. } => Some(TypeLayout {
                size_bytes: self.target.pointer_bytes(),
                align_bytes: self.target.pointer_bytes(),
            }),
            HirType::Slice(_) => Some(TypeLayout {
                size_bytes: self
                    .target
                    .pointer_bytes()
                    .checked_add(self.target.stack_slot_bytes())?,
                align_bytes: self.target.pointer_bytes().max(1),
            }),
            HirType::Array { length, element } => {
                let element = self.layout_inner(element, visiting)?;
                Some(TypeLayout {
                    size_bytes: length.checked_mul(element.size_bytes)?,
                    align_bytes: element.align_bytes,
                })
            }
            HirType::Named(name) if self.structs.contains_key(name) => {
                let key = format!("struct:{name}");
                if !visiting.insert(key.clone()) {
                    return None;
                }
                let slot_bytes = self.target.stack_slot_bytes();
                if slot_bytes == 0 {
                    return None;
                }
                let mut size = 0u64;
                // MIR currently places each struct field on a stack-slot boundary.
                for (_, field_type) in self.structs.get(name)? {
                    let field = self.layout_inner(field_type, visiting)?;
                    size = size.checked_add(self.round_to_slot(field.size_bytes)?)?;
                }
                visiting.remove(&key);
                Some(TypeLayout {
                    size_bytes: size,
                    align_bytes: slot_bytes,
                })
            }
            HirType::Named(name) if self.enums.contains_key(name) => {
                let variants = self.enums.get(name)?;
                if variants.iter().all(Option::is_none) {
                    return Some(TypeLayout {
                        size_bytes: self.target.pointer_bytes(),
                        align_bytes: self.target.pointer_bytes().max(1),
                    });
                }
                let key = format!("enum:{name}");
                if !visiting.insert(key.clone()) {
                    return None;
                }
                let mut payload_slots = 1u64;
                for payload in variants.iter().flatten() {
                    payload_slots = payload_slots.max(self.slots_for_value(payload)? as u64);
                }
                visiting.remove(&key);
                let size = payload_slots
                    .checked_add(1)?
                    .checked_mul(self.target.stack_slot_bytes())?;
                Some(TypeLayout {
                    size_bytes: size,
                    align_bytes: self.target.stack_slot_bytes(),
                })
            }
            HirType::Named(_) => Some(TypeLayout {
                size_bytes: self.target.pointer_bytes(),
                align_bytes: self.target.pointer_bytes(),
            }),
            _ => None,
        }
    }
}

pub(crate) fn scalar_storage_size(data_type: &HirType) -> Option<u8> {
    match data_type {
        HirType::Integer { bits, .. } | HirType::Float { bits } => match bits {
            8 => Some(1),
            16 => Some(2),
            32 => Some(4),
            64 => Some(8),
            _ => None,
        },
        HirType::Bool => Some(1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{AbiClass, TypeLayouts};
    use crate::hir::HirType;
    use crate::lir::TargetDataLayout;
    use std::collections::HashMap;

    fn integer(bits: u16) -> HirType {
        HirType::Integer {
            bits,
            signed: false,
        }
    }

    #[test]
    fn lays_out_nested_structs_arrays_and_slice_fields() {
        let structs = HashMap::from([
            (
                "inner".to_string(),
                vec![
                    ("tag".to_string(), integer(8)),
                    ("value".to_string(), integer(32)),
                ],
            ),
            (
                "holder".to_string(),
                vec![
                    (
                        "items".to_string(),
                        HirType::Array {
                            length: 2,
                            element: Box::new(HirType::Named("inner".to_string())),
                        },
                    ),
                    ("view".to_string(), HirType::Slice(Box::new(integer(8)))),
                    ("tail".to_string(), integer(64)),
                ],
            ),
        ]);
        let layouts = TypeLayouts::new(TargetDataLayout::HOST_64, structs, HashMap::new());
        let holder = HirType::Named("holder".to_string());

        assert_eq!(
            layouts
                .layout(&HirType::Named("inner".into()))
                .unwrap()
                .size_bytes,
            16
        );
        assert_eq!(layouts.layout(&holder).unwrap().size_bytes, 56);
        assert_eq!(
            layouts.struct_field_offset_slots("holder", "items"),
            Some(0)
        );
        assert_eq!(layouts.struct_field_offset_slots("holder", "view"), Some(4));
        assert_eq!(layouts.struct_field_offset_slots("holder", "tail"), Some(6));
        assert_eq!(
            layouts.abi_class(&holder),
            Some(AbiClass::IndirectAggregate { slots: 7 })
        );
    }

    #[test]
    fn lays_out_enum_payloads_and_classifies_values_for_the_abi() {
        let enums = HashMap::from([
            (
                "choice".to_string(),
                vec![
                    None,
                    Some(HirType::Array {
                        length: 3,
                        element: Box::new(integer(16)),
                    }),
                ],
            ),
            (
                "view_or_none".to_string(),
                vec![None, Some(HirType::Slice(Box::new(integer(8))))],
            ),
        ]);
        let layouts = TypeLayouts::new(TargetDataLayout::HOST_64, HashMap::new(), enums);

        assert_eq!(
            layouts
                .layout(&HirType::Named("choice".into()))
                .unwrap()
                .size_bytes,
            16
        );
        assert_eq!(layouts.enum_storage_slot_count("view_or_none"), Some(3));
        assert_eq!(
            layouts.abi_class(&HirType::Slice(Box::new(integer(8)))),
            Some(AbiClass::SlicePair)
        );
        assert_eq!(
            layouts.abi_class(&HirType::Float { bits: 32 }),
            Some(AbiClass::Float(32))
        );
    }

    #[test]
    fn respects_target_pointer_width_in_slice_storage() {
        let layouts = TypeLayouts::new(
            TargetDataLayout::new(4, 8),
            HashMap::from([(
                "holder".to_string(),
                vec![
                    ("view".to_string(), HirType::Slice(Box::new(integer(8)))),
                    ("value".to_string(), integer(32)),
                ],
            )]),
            HashMap::new(),
        );
        let slice = HirType::Slice(Box::new(integer(8)));

        assert_eq!(layouts.layout(&slice).unwrap().size_bytes, 12);
        assert_eq!(
            layouts
                .layout(&HirType::Named("holder".into()))
                .unwrap()
                .size_bytes,
            24
        );
        assert_eq!(
            layouts.struct_field_offset_slots("holder", "value"),
            Some(2)
        );
    }

    #[test]
    fn classifies_every_supported_scalar_and_pointer_sized_type_on_32_and_64_bit_layouts() {
        let types = [
            integer(8),
            integer(16),
            integer(32),
            integer(64),
            HirType::Float { bits: 16 },
            HirType::Float { bits: 32 },
            HirType::Float { bits: 64 },
            HirType::Bool,
            HirType::Pointer(Box::new(integer(8))),
            HirType::Function {
                params: vec![integer(32)],
                result: Box::new(integer(32)),
            },
            HirType::Slice(Box::new(integer(8))),
            HirType::Array {
                length: 3,
                element: Box::new(integer(16)),
            },
        ];

        for (pointer_bytes, stack_slot_bytes) in [(4, 4), (8, 8)] {
            let layouts = TypeLayouts::new(
                TargetDataLayout::new(pointer_bytes, stack_slot_bytes),
                HashMap::new(),
                HashMap::new(),
            );
            for data_type in &types {
                assert!(layouts.layout(data_type).is_some(), "{data_type:?}");
                assert!(layouts.abi_class(data_type).is_some(), "{data_type:?}");
            }
            assert_eq!(
                layouts
                    .layout(&HirType::Pointer(Box::new(integer(8))))
                    .unwrap()
                    .size_bytes,
                pointer_bytes
            );
            assert_eq!(
                layouts
                    .layout(&HirType::Slice(Box::new(integer(8))))
                    .unwrap()
                    .size_bytes,
                pointer_bytes + stack_slot_bytes
            );
        }
    }
}
