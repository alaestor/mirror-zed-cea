---@meta

---@class MonoHandle: integer
---@class MonoType: integer
---@class ClassId: MonoHandle
---@class MethodId: MonoHandle
---@class FieldId: MonoHandle
---@class MonoTypeId: MonoHandle
---@class AssemblyId: MonoHandle
---@class AssemblyImage: MonoHandle
---@class DomainId: MonoHandle
---@class MonoObject: Address
---@class MethodAttribute: integer
---@class FieldAttribute: integer
---@class MonoCommand: integer

---@class MonoMethodParameter
---@field name string
---@field monotype MonoTypeId
---@field type MonoType

---@class MonoMethodParameters
---@field parameters MonoMethodParameter[]
---@field returnmonotype MonoTypeId
---@field returntype MonoType

---@class MonoClassInfo
---@field Handle ClassId
---@field FullName string
---@field NameSpace string

---@class MonoMethodInfo
---@field method MethodId
---@field name string
---@field flags MethodAttribute
---@field parent ClassId?
---@field address Address?

---@class MonoFieldInfo
---@field field FieldId
---@field type MonoTypeId
---@field monotype MonoType
---@field parent ClassId
---@field offset integer
---@field flags FieldAttribute
---@field name string
---@field typename string
---@field isStatic boolean
---@field isConst boolean
---@field staticAddress Address?
---@field altname string?

---@type MonoType
MONO_TYPE_END = 0x00
---@type MonoType
MONO_TYPE_VOID = 0x01
---@type MonoType
MONO_TYPE_BOOLEAN = 0x02
---@type MonoType
MONO_TYPE_CHAR = 0x03
---@type MonoType
MONO_TYPE_I1 = 0x04
---@type MonoType
MONO_TYPE_U1 = 0x05
---@type MonoType
MONO_TYPE_I2 = 0x06
---@type MonoType
MONO_TYPE_U2 = 0x07
---@type MonoType
MONO_TYPE_I4 = 0x08
---@type MonoType
MONO_TYPE_U4 = 0x09
---@type MonoType
MONO_TYPE_I8 = 0x0a
---@type MonoType
MONO_TYPE_U8 = 0x0b
---@type MonoType
MONO_TYPE_R4 = 0x0c
---@type MonoType
MONO_TYPE_R8 = 0x0d
---@type MonoType
MONO_TYPE_STRING = 0x0e
---@type MonoType
MONO_TYPE_PTR = 0x0f
---@type MonoType
MONO_TYPE_BYREF = 0x10
---@type MonoType
MONO_TYPE_VALUETYPE = 0x11
---@type MonoType
MONO_TYPE_CLASS = 0x12
---@type MonoType
MONO_TYPE_VAR = 0x13
---@type MonoType
MONO_TYPE_ARRAY = 0x14
---@type MonoType
MONO_TYPE_GENERICINST = 0x15
---@type MonoType
MONO_TYPE_TYPEDBYREF = 0x16
---@type MonoType
MONO_TYPE_I = 0x18
---@type MonoType
MONO_TYPE_U = 0x19
---@type MonoType
MONO_TYPE_FNPTR = 0x1b
---@type MonoType
MONO_TYPE_OBJECT = 0x1c
---@type MonoType
MONO_TYPE_SZARRAY = 0x1d
---@type MonoType
MONO_TYPE_MVAR = 0x1e
---@type MonoType
MONO_TYPE_CMOD_REQD = 0x1f
---@type MonoType
MONO_TYPE_CMOD_OPT = 0x20
---@type MonoType
MONO_TYPE_INTERNAL = 0x21
---@type MonoType
MONO_TYPE_MODIFIER = 0x40
---@type MonoType
MONO_TYPE_SENTINEL = 0x41
---@type MonoType
MONO_TYPE_PINNED = 0x45
---@type MonoType
MONO_TYPE_ENUM = 0x55

---@type table<MonoType, VirtualType>
monoTypeToVartypeLookup = {}
---@type table<MonoType, string>
monoTypeToCStringLookup = {}

---@type FieldAttribute
FIELD_ATTRIBUTE_FIELD_ACCESS_MASK = 0x0007
---@type FieldAttribute
FIELD_ATTRIBUTE_COMPILER_CONTROLLED = 0x0000
---@type FieldAttribute
FIELD_ATTRIBUTE_PRIVATE = 0x0001
---@type FieldAttribute
FIELD_ATTRIBUTE_FAM_AND_ASSEM = 0x0002
---@type FieldAttribute
FIELD_ATTRIBUTE_ASSEMBLY = 0x0003
---@type FieldAttribute
FIELD_ATTRIBUTE_FAMILY = 0x0004
---@type FieldAttribute
FIELD_ATTRIBUTE_FAM_OR_ASSEM = 0x0005
---@type FieldAttribute
FIELD_ATTRIBUTE_PUBLIC = 0x0006
---@type FieldAttribute
FIELD_ATTRIBUTE_STATIC = 0x0010
---@type FieldAttribute
FIELD_ATTRIBUTE_INIT_ONLY = 0x0020
---@type FieldAttribute
FIELD_ATTRIBUTE_LITERAL = 0x0040
---@type FieldAttribute
FIELD_ATTRIBUTE_NOT_SERIALIZED = 0x0080
---@type FieldAttribute
FIELD_ATTRIBUTE_HAS_FIELD_RVA = 0x0100
---@type FieldAttribute
FIELD_ATTRIBUTE_SPECIAL_NAME = 0x0200
---@type FieldAttribute
FIELD_ATTRIBUTE_RT_SPECIAL_NAME = 0x0400
---@type FieldAttribute
FIELD_ATTRIBUTE_HAS_FIELD_MARSHAL = 0x1000
---@type FieldAttribute
FIELD_ATTRIBUTE_PINVOKE_IMPL = 0x2000
---@type FieldAttribute
FIELD_ATTRIBUTE_HAS_DEFAULT = 0x8000
---@type FieldAttribute
FIELD_ATTRIBUTE_RESERVED_MASK = 0x9500

---@type MethodAttribute
METHOD_ATTRIBUTE_MEMBER_ACCESS_MASK = 0x0007
---@type MethodAttribute
METHOD_ATTRIBUTE_COMPILER_CONTROLLED = 0x0000
---@type MethodAttribute
METHOD_ATTRIBUTE_PRIVATE = 0x0001
---@type MethodAttribute
METHOD_ATTRIBUTE_FAM_AND_ASSEM = 0x0002
---@type MethodAttribute
METHOD_ATTRIBUTE_ASSEM = 0x0003
---@type MethodAttribute
METHOD_ATTRIBUTE_FAMILY = 0x0004
---@type MethodAttribute
METHOD_ATTRIBUTE_FAM_OR_ASSEM = 0x0005
---@type MethodAttribute
METHOD_ATTRIBUTE_PUBLIC = 0x0006
---@type MethodAttribute
METHOD_ATTRIBUTE_UNMANAGED_EXPORT = 0x0008
---@type MethodAttribute
METHOD_ATTRIBUTE_STATIC = 0x0010
---@type MethodAttribute
METHOD_ATTRIBUTE_FINAL = 0x0020
---@type MethodAttribute
METHOD_ATTRIBUTE_VIRTUAL = 0x0040
---@type MethodAttribute
METHOD_ATTRIBUTE_HIDE_BY_SIG = 0x0080
---@type MethodAttribute
METHOD_ATTRIBUTE_VTABLE_LAYOUT_MASK = 0x0100
---@type MethodAttribute
METHOD_ATTRIBUTE_REUSE_SLOT = 0x0000
---@type MethodAttribute
METHOD_ATTRIBUTE_NEW_SLOT = 0x0100
---@type MethodAttribute
METHOD_ATTRIBUTE_STRICT = 0x0200
---@type MethodAttribute
METHOD_ATTRIBUTE_ABSTRACT = 0x0400
---@type MethodAttribute
METHOD_ATTRIBUTE_SPECIAL_NAME = 0x0800
---@type MethodAttribute
METHOD_ATTRIBUTE_PINVOKE_IMPL = 0x2000

---@type MonoCommand
MONOCMD_INVOKEMETHOD = 1

---@return boolean
function mono_initialize() end

---@return boolean
function LaunchMonoDataCollector() end

---@return boolean
function mono_isValid() end

---@return any result
---@return MonoType type
function mono_readObject() end

---@param method MethodId
---@return MethodAttribute
function mono_method_getFlags(method) end

---@param method MethodId
---@return MonoMethodParameters?
function mono_method_get_parameters(method) end

---@param method MethodId
---@return Address
function mono_compile_method(method) end

---@param domain DomainId?
---@param method MethodId
---@param object MonoObject
---@param args table
---@return any result
---@return string? exception
---@return MonoType? type
function mono_invoke_method(domain, method, object, args) end

---@param method MethodId
---@return ClassId
function mono_method_getClass(method) end

---@param class ClassId
---@return MonoTypeId
function mono_class_get_type(class) end

---@param object MonoObject
---@return MonoObject
function mono_object_unbox(object) end

---@param object MonoObject
---@return table<string, any>?
function mono_object_enumValues(object) end

---@param assembly AssemblyId
---@return AssemblyImage?
function mono_getImageFromAssembly(assembly) end

---@param image AssemblyImage
---@return string? name
---@return string? error
function mono_image_get_name(image) end

---@param image AssemblyImage
---@return MonoClassInfo[]
function mono_image_enumClassesEx(image) end

---@return AssemblyId[]
function mono_enumAssemblies() end

---@param method MethodId
---@return string
function mono_method_getName(method) end

---@param method MethodId
---@return string parameterTypes
---@return string[] parameterNames
---@return string returnType
function mono_method_getSignature(method) end

---@param method MethodId
---@return string
function mono_method_getFullName(method) end

---@param types string Comma-separated Mono type names
---@return string[]
function mono_splitParameters(types) end

---@param class ClassId
---@return ClassId
function mono_class_getParent(class) end

---@param class ClassId
---@return string
function mono_class_getName(class) end

---@param class ClassId
---@return string
function mono_class_getNamespace(class) end

---@param class ClassId
---@param includeParents boolean?
---@return MonoMethodInfo[]
function mono_class_enumMethods(class, includeParents) end

---@param class ClassId
---@param includeParents boolean?
---@param expandedStructs boolean?
---@return MonoFieldInfo[]
function mono_class_enumFields(class, includeParents, expandedStructs) end

---@param fields MonoFieldInfo[]
---@return integer
function mono_structfields_getStartOffset(fields) end

---@param class ClassId
---@param field FieldId
---@return any
function mono_class_getStaticFieldValue(class, field) end

---@param vartype VirtualType
---@param value any
---@return boolean
function mono_writeObject(vartype, value) end

---@param monotype MonoTypeId
---@return MonoType
function mono_type_get_type(monotype) end
