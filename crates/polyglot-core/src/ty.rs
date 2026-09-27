//! The C type model used across parsing, checking and codegen.

use std::collections::HashMap;
use std::fmt;

/// Supported integer kinds. Floating point is not supported in this
/// release (see README limitations).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntKind {
    /// `char` / `signed char` — 8 bits.
    Char,
    /// `int`, `unsigned int`, `short` — 32 bits.
    Int,
    /// `long`, `unsigned long`, `long long` — 64 bits.
    Long,
}

impl IntKind {
    pub fn size(self) -> u64 {
        match self {
            IntKind::Char => 1,
            IntKind::Int => 4,
            IntKind::Long => 8,
        }
    }

    pub fn align(self) -> u64 {
        self.size()
    }
}

/// A field of a struct definition.
#[derive(Clone, Debug)]
pub struct StructField {
    pub name: String,
    pub ty: Type,
    /// Byte offset from the start of the struct; computed by the
    /// type checker once the layout is known (`0` before that).
    pub offset: u64,
}

/// A struct definition (`struct Point { int x; int y; };`).
#[derive(Clone, Debug)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<StructField>,
    /// Total size in bytes including tail padding; computed by the
    /// type checker (`0` for incomplete/forward declarations).
    pub size: u64,
    pub align: u64,
}

impl StructDef {
    /// Compute field offsets, `size` and `align` for this definition.
    ///
    /// Layout is the classic System V scheme: every field is aligned
    /// to its own alignment, the total size is rounded up to the
    /// struct's alignment.
    ///
    /// `struct_table` is used to look up the layout of nested struct types.
    pub fn layout(&mut self, struct_table: &HashMap<String, StructDef>) {
        let mut offset = 0u64;
        let mut max_align = 1u64;
        for field in &mut self.fields {
            let falign = field.ty.align_with_structs(struct_table);
            max_align = max_align.max(falign);
            offset = round_up(offset, falign);
            field.offset = offset;
            offset += field.ty.size_with_structs(struct_table);
        }
        self.align = max_align;
        self.size = round_up(offset, max_align);
    }

    pub fn field(&self, name: &str) -> Option<(u64, &Type)> {
        self.fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| (f.offset, &f.ty))
    }
}

pub(crate) fn round_up(value: u64, align: u64) -> u64 {
    if align == 0 {
        return value;
    }
    value.div_ceil(align) * align
}

/// A C type.
///
/// Types are structural except structs which are nominal (matched by
/// name); the type checker owns the table of [`StructDef`]s.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Void,
    Int(IntKind),
    Pointer(Box<Type>),
    /// Fixed-size array. The length is `None` only between parsing and
    /// initialization analysis (e.g. `int a[] = {1,2,3};`).
    Array(Box<Type>, Option<u64>),
    Struct(String),
}

impl Type {
    pub fn int() -> Type {
        Type::Int(IntKind::Int)
    }
    pub fn long() -> Type {
        Type::Int(IntKind::Long)
    }
    pub fn char_t() -> Type {
        Type::Int(IntKind::Char)
    }
    pub fn ptr_to(inner: Type) -> Type {
        Type::Pointer(Box::new(inner))
    }
    pub fn void_ptr() -> Type {
        Type::ptr_to(Type::Void)
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Type::Int(_))
    }

    pub fn is_pointer(&self) -> bool {
        matches!(self, Type::Pointer(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Type::Array(..))
    }

    /// Scalars can appear in conditions and arithmetic.
    pub fn is_scalar(&self) -> bool {
        matches!(self, Type::Int(_) | Type::Pointer(_) | Type::Array(..))
    }

    /// The type you get when you index or dereference one level off
    /// this type (arrays decay to their element type).
    pub fn pointee(&self) -> Option<&Type> {
        match self {
            Type::Pointer(inner) | Type::Array(inner, _) => Some(inner),
            _ => None,
        }
    }

    pub fn size(&self) -> u64 {
        self.size_with_structs(&HashMap::new())
    }

    pub fn align(&self) -> u64 {
        self.align_with_structs(&HashMap::new())
    }

    /// Size with struct table for nested struct lookups.
    pub fn size_with_structs(&self, struct_table: &HashMap<String, StructDef>) -> u64 {
        match self {
            Type::Void => 0,
            Type::Int(k) => k.size(),
            Type::Pointer(_) => 8,
            Type::Array(elem, len) => len.unwrap_or(0) * elem.size_with_structs(struct_table),
            Type::Struct(name) => struct_table.get(name).map(|s| s.size).unwrap_or(0),
        }
    }

    /// Alignment with struct table for nested struct lookups.
    pub fn align_with_structs(&self, struct_table: &HashMap<String, StructDef>) -> u64 {
        match self {
            Type::Void => 0,
            Type::Int(k) => k.align(),
            Type::Pointer(_) => 8,
            Type::Array(elem, _) => elem.align_with_structs(struct_table),
            Type::Struct(name) => struct_table.get(name).map(|s| s.align).unwrap_or(8),
        }
    }

    pub fn is_void(&self) -> bool {
        matches!(self, Type::Void)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Void => write!(f, "void"),
            Type::Int(IntKind::Char) => write!(f, "char"),
            Type::Int(IntKind::Int) => write!(f, "int"),
            Type::Int(IntKind::Long) => write!(f, "long"),
            Type::Pointer(inner) => write!(f, "{inner}*"),
            Type::Array(elem, len) => match len {
                Some(n) => write!(f, "{elem}[{n}]"),
                None => write!(f, "{elem}[]"),
            },
            Type::Struct(name) => write!(f, "struct {name}"),
        }
    }
}
