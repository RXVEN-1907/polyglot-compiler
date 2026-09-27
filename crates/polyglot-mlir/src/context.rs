use anyhow::{Context, Result};
use mlir_sys::*;
use std::ffi::CString;
use tracing::debug;

/// MLIR Context wrapper
pub struct MlirContextWrapper {
    context: MlirContext,
    dialects_loaded: bool,
}

impl MlirContextWrapper {
    /// Create a new MLIR context with standard dialects registered
    pub fn new() -> Result<Self> {
        unsafe {
            let context = mlirContextCreate();

            let mut this = Self {
                context,
                dialects_loaded: false,
            };

            this.load_standard_dialects()?;
            Ok(this)
        }
    }

    /// Load standard MLIR dialects
    fn load_standard_dialects(&mut self) -> Result<()> {
        unsafe {
            // Create dialect registry
            let registry = mlirDialectRegistryCreate();

            // Register dialect handles
            mlirGetDialectHandle__arith__();
            mlirGetDialectHandle__func__();
            mlirGetDialectHandle__memref__();
            mlirGetDialectHandle__scf__();
            mlirGetDialectHandle__vector__();
            mlirGetDialectHandle__llvm__();
            mlirGetDialectHandle__cf__();

            // Insert all into registry
            mlirDialectHandleInsertDialect(mlirGetDialectHandle__arith__(), registry);
            mlirDialectHandleInsertDialect(mlirGetDialectHandle__func__(), registry);
            mlirDialectHandleInsertDialect(mlirGetDialectHandle__memref__(), registry);
            mlirDialectHandleInsertDialect(mlirGetDialectHandle__scf__(), registry);
            mlirDialectHandleInsertDialect(mlirGetDialectHandle__vector__(), registry);
            mlirDialectHandleInsertDialect(mlirGetDialectHandle__llvm__(), registry);
            mlirDialectHandleInsertDialect(mlirGetDialectHandle__cf__(), registry);

            // Append registry to context
            mlirContextAppendDialectRegistry(self.context, registry);

            // Load all dialects
            mlirContextLoadAllAvailableDialects(self.context);

            // Clean up registry
            mlirDialectRegistryDestroy(registry);

            self.dialects_loaded = true;
            debug!("Loaded standard MLIR dialects");
            Ok(())
        }
    }

    /// Get raw context pointer
    pub fn as_ptr(&self) -> MlirContext {
        self.context
    }

    /// Parse an MLIR module from string
    pub fn parse_module(&self, mlir_text: &str) -> Result<MlirModuleWrapper> {
        unsafe {
            let cstr = CString::new(mlir_text).context("Failed to create C string")?;
            let string_ref = mlirStringRefCreateFromCString(cstr.as_ptr());
            let module = mlirModuleCreateParse(self.context, string_ref);

            if module.ptr.is_null() {
                anyhow::bail!("Failed to parse MLIR module");
            }

            Ok(MlirModuleWrapper {
                module,
                context: self.context,
            })
        }
    }

    /// Create an empty module
    pub fn create_module(&self, _name: &str) -> Result<MlirModuleWrapper> {
        unsafe {
            let loc = mlirLocationUnknownGet(self.context);
            let module = mlirModuleCreateEmpty(loc);

            if module.ptr.is_null() {
                anyhow::bail!("Failed to create MLIR module");
            }

            Ok(MlirModuleWrapper {
                module,
                context: self.context,
            })
        }
    }

    /// Run optimization pipeline on a module
    pub fn optimize_module(
        &self,
        module: &MlirModuleWrapper,
        opt_level: crate::types::OptLevel,
    ) -> Result<()> {
        unsafe {
            let pass_manager = mlirPassManagerCreate(self.context);
            if pass_manager.ptr.is_null() {
                anyhow::bail!("Failed to create pass manager");
            }

            // Add standard optimization passes based on opt level
            match opt_level {
                crate::types::OptLevel::None => {
                    // No optimizations
                }
                crate::types::OptLevel::Less => {
                    let canonicalizer = mlirCreateTransformsCanonicalizer();
                    mlirPassManagerAddOwnedPass(pass_manager, canonicalizer);
                    let cse = mlirCreateTransformsCSE();
                    mlirPassManagerAddOwnedPass(pass_manager, cse);
                }
                crate::types::OptLevel::Default => {
                    let canonicalizer = mlirCreateTransformsCanonicalizer();
                    mlirPassManagerAddOwnedPass(pass_manager, canonicalizer);
                    let cse = mlirCreateTransformsCSE();
                    mlirPassManagerAddOwnedPass(pass_manager, cse);
                    let licm = mlirCreateTransformsLoopInvariantCodeMotion();
                    mlirPassManagerAddOwnedPass(pass_manager, licm);
                    let inliner = mlirCreateTransformsInliner();
                    mlirPassManagerAddOwnedPass(pass_manager, inliner);
                }
                crate::types::OptLevel::Aggressive => {
                    let canonicalizer = mlirCreateTransformsCanonicalizer();
                    mlirPassManagerAddOwnedPass(pass_manager, canonicalizer);
                    let cse = mlirCreateTransformsCSE();
                    mlirPassManagerAddOwnedPass(pass_manager, cse);
                    let licm = mlirCreateTransformsLoopInvariantCodeMotion();
                    mlirPassManagerAddOwnedPass(pass_manager, licm);
                    let inliner = mlirCreateTransformsInliner();
                    mlirPassManagerAddOwnedPass(pass_manager, inliner);
                }
                crate::types::OptLevel::Size => {
                    let canonicalizer = mlirCreateTransformsCanonicalizer();
                    mlirPassManagerAddOwnedPass(pass_manager, canonicalizer);
                    let cse = mlirCreateTransformsCSE();
                    mlirPassManagerAddOwnedPass(pass_manager, cse);
                    let inliner = mlirCreateTransformsInliner();
                    mlirPassManagerAddOwnedPass(pass_manager, inliner);
                }
                crate::types::OptLevel::SizeAggressive => {
                    let canonicalizer = mlirCreateTransformsCanonicalizer();
                    mlirPassManagerAddOwnedPass(pass_manager, canonicalizer);
                    let cse = mlirCreateTransformsCSE();
                    mlirPassManagerAddOwnedPass(pass_manager, cse);
                    let inliner = mlirCreateTransformsInliner();
                    mlirPassManagerAddOwnedPass(pass_manager, inliner);
                }
            }

            // Run the pass manager on the module operation
            let op = mlirModuleGetOperation(module.module);
            let result = mlirPassManagerRunOnOp(pass_manager, op);
            mlirPassManagerDestroy(pass_manager);

            if result.value != 0 {
                anyhow::bail!("MLIR optimization failed");
            }

            debug!("MLIR optimization completed");
            Ok(())
        }
    }

    /// Print module to string
    pub fn module_to_string(&self, module: &MlirModuleWrapper) -> Result<String> {
        unsafe {
            let op = mlirModuleGetOperation(module.module);
            let mut result = String::new();

            extern "C" fn string_callback(
                str_ref: MlirStringRef,
                user_data: *mut std::ffi::c_void,
            ) {
                let result = unsafe { &mut *(user_data as *mut String) };
                let slice = unsafe {
                    std::slice::from_raw_parts(str_ref.data as *const u8, str_ref.length)
                };
                if let Ok(s) = std::str::from_utf8(slice) {
                    result.push_str(s);
                }
            }

            let user_data = &mut result as *mut String as *mut std::ffi::c_void;
            mlirOperationPrint(op, Some(string_callback), user_data);

            Ok(result)
        }
    }

    /// Verify module
    pub fn verify_module(&self, module: &MlirModuleWrapper) -> Result<bool> {
        unsafe {
            let op = mlirModuleGetOperation(module.module);
            // Enable verifier in pass manager
            let pm = mlirPassManagerCreate(self.context);
            mlirPassManagerEnableVerifier(pm, true);
            let result = mlirPassManagerRunOnOp(pm, op);
            mlirPassManagerDestroy(pm);
            Ok(result.value == 0)
        }
    }
}

impl Drop for MlirContextWrapper {
    fn drop(&mut self) {
        unsafe {
            mlirContextDestroy(self.context);
        }
    }
}

/// MLIR Module wrapper
pub struct MlirModuleWrapper {
    module: MlirModule,
    context: MlirContext,
}

impl MlirModuleWrapper {
    pub fn as_ptr(&self) -> MlirModule {
        self.module
    }

    pub fn context(&self) -> MlirContext {
        self.context
    }

    /// Get module name
    pub fn name(&self) -> Result<String> {
        unsafe {
            let op = mlirModuleGetOperation(self.module);
            let identifier = mlirOperationGetName(op);
            let str_ref = mlirIdentifierStr(identifier);
            if str_ref.length == 0 {
                Ok(String::new())
            } else {
                let slice = std::slice::from_raw_parts(str_ref.data as *const u8, str_ref.length);
                Ok(std::str::from_utf8(slice).unwrap_or("").to_string())
            }
        }
    }

    /// Dump module for debugging
    pub fn dump(&self) {
        unsafe {
            let op = mlirModuleGetOperation(self.module);
            extern "C" fn dump_callback(str_ref: MlirStringRef, _user_data: *mut std::ffi::c_void) {
                let slice = unsafe {
                    std::slice::from_raw_parts(str_ref.data as *const u8, str_ref.length)
                };
                if let Ok(s) = std::str::from_utf8(slice) {
                    eprint!("{}", s);
                }
            }
            mlirOperationPrint(op, Some(dump_callback), std::ptr::null_mut());
        }
    }
}

impl Drop for MlirModuleWrapper {
    fn drop(&mut self) {
        unsafe {
            mlirModuleDestroy(self.module);
        }
    }
}

/// MLIR types
pub mod types {
    use mlir_sys::*;

    /// Create an integer type
    pub fn integer_type(context: MlirContext, width: u32) -> MlirType {
        unsafe { mlirIntegerTypeGet(context, width) }
    }

    /// Create a function type
    pub fn function_type(
        context: MlirContext,
        inputs: &[MlirType],
        results: &[MlirType],
    ) -> MlirType {
        unsafe {
            mlirFunctionTypeGet(
                context,
                inputs.len() as isize,
                inputs.as_ptr(),
                results.len() as isize,
                results.as_ptr(),
            )
        }
    }

    /// Create a pointer type (LLVM dialect)
    pub fn pointer_type(_context: MlirContext, pointee: MlirType) -> MlirType {
        unsafe { mlirLLVMPointerTypeGet(pointee, 0) }
    }
}
