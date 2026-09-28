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

/// Bit-field placement within a struct or union, filled in by
/// [`StructDef::layout`]. The declared type (`ty`) doubles as the
/// storage-unit type: its alignment is the unit width the field packs
/// into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitfieldLayout {
    /// Position of the least significant bit of the field within its
    /// storage unit (little-endian bit order, matching x86-64).
    pub lsb: u32,
    /// Width of the field in bits; `0` marks an unnamed `:0` boundary
    /// (which occupies no storage and is never accessed).
    pub width: u32,
    /// True when declared `signed` (reads sign-extend). The frontend's
    /// integer model is otherwise unsignedness-free, so this is
    /// recorded only for bit-fields.
    pub signed: bool,
}

/// A field of a struct or union definition. Unnamed bit-fields use the
/// empty string as their name; it is not a valid identifier, so it can
/// never collide with a user field and member lookup never finds it.
#[derive(Clone, Debug)]
pub struct StructField {
    pub name: String,
    pub ty: Type,
    /// Byte offset from the start of the struct; for bit-fields, the
    /// byte offset of the containing storage unit. Computed by
    /// [`StructDef::layout`] (`0` before that).
    pub offset: u64,
    /// Present for bit-field members (`unsigned f : 3;`).
    pub bitfield: Option<BitfieldLayout>,
}

/// A struct or union definition (`struct Point { int x; int y; };`).
///
/// Unions are represented with the same type: every member sits at
/// offset 0 and the size is the largest member rounded to the
/// alignment.
#[derive(Clone, Debug)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<StructField>,
    /// Total size in bytes including tail padding; computed by the
    /// type checker (`0` for incomplete/forward declarations).
    pub size: u64,
    pub align: u64,
    /// True for `union` definitions; affects layout only.
    pub is_union: bool,
}

impl StructDef {
    /// Compute field offsets, `size` and `align` for this definition.
    ///
    /// Plain structs use the classic System V scheme: every field is
    /// aligned to its own alignment, the total size is rounded up to
    /// the struct's alignment.
    ///
    /// Bit-fields follow the ELF x86-64 psABI: a field of declared
    /// type `T` and width `n` packs into the current `sizeof(T)`-aligned
    /// storage unit when it fits, otherwise a fresh unit is started;
    /// unnamed `:0` fields close the current unit. Named bit-fields
    /// contribute their declared type's alignment to the struct;
    /// unnamed ones (including `:0`) contribute nothing.
    ///
    /// Unions place every member at offset 0 (so member accesses
    /// overlap, which is exactly C type punning); the size is the
    /// largest member rounded up to the alignment.
    ///
    /// `struct_table` is used to look up the layout of nested struct types.
    pub fn layout(&mut self, struct_table: &HashMap<String, StructDef>) {
        if self.is_union {
            self.layout_union(struct_table);
            return;
        }
        // Bit cursor: allocation position in bits from the struct start.
        let mut bit = 0u64;
        let mut max_align = 1u64;
        for field in &mut self.fields {
            if let Some(bf) = field.bitfield {
                let unit_bits = field.ty.align_with_structs(struct_table) * 8;
                if bf.width == 0 {
                    // `:0`: move to the next storage-unit boundary.
                    bit = round_up(bit, unit_bits);
                    field.offset = bit / 8;
                    continue;
                }
                if bit % unit_bits + bf.width as u64 > unit_bits {
                    bit = round_up(bit, unit_bits);
                }
                let unit_start = bit - bit % unit_bits;
                let lsb = (bit - unit_start) as u32;
                field.offset = unit_start / 8;
                field.bitfield = Some(BitfieldLayout { lsb, ..bf });
                bit += bf.width as u64;
                // Only named bit-fields affect the struct's alignment;
                // unnamed padding (including `:0`) does not.
                if !field.name.is_empty() {
                    max_align = max_align.max(field.ty.align_with_structs(struct_table));
                }
            } else {
                let falign = field.ty.align_with_structs(struct_table);
                max_align = max_align.max(falign);
                bit = round_up(bit, falign * 8);
                field.offset = bit / 8;
                bit += field.ty.size_with_structs(struct_table) * 8;
            }
        }
        self.align = max_align;
        self.size = round_up(bit.div_ceil(8), max_align);
    }

    /// Union layout: all members overlap at offset 0.
    fn layout_union(&mut self, struct_table: &HashMap<String, StructDef>) {
        let mut max_size = 0u64;
        let mut max_align = 1u64;
        for field in &mut self.fields {
            match field.bitfield {
                // Unnamed bit-fields contribute nothing in a union
                // (verified against gcc/clang on x86-64).
                Some(_) if field.name.is_empty() => {
                    field.offset = 0;
                }
                // A named bit-field occupies a whole storage unit of
                // its declared type, starting at bit 0.
                Some(bf) => {
                    let unit = field.ty.align_with_structs(struct_table);
                    field.offset = 0;
                    field.bitfield = Some(BitfieldLayout { lsb: 0, ..bf });
                    max_size = max_size.max(unit);
                    max_align = max_align.max(unit);
                }
                None => {
                    let falign = field.ty.align_with_structs(struct_table);
                    max_size = max_size.max(field.ty.size_with_structs(struct_table));
                    max_align = max_align.max(falign);
                    field.offset = 0;
                }
            }
        }
        self.align = max_align;
        self.size = round_up(max_size, max_align);
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
