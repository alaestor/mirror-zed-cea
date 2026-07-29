---@meta

---@class Array<T>: table<integer, T>
---@class Dict<T>: table<string, T>
---@class Address: integer Address in the target process, including zero (nullptr)
---@class Pointer: Address Address whose value represents another address
---@class Offset: integer Offset used in a pointer path
---@class AddressExpression: string Address, symbol, module, or pointer expression understood by CE
---@class SymbolName: string Name registered with CE's symbol handler
---@class AOBPattern: string Space-separated array-of-bytes pattern; wildcards are accepted
---@class MemoryProtection: integer Protection flags accepted by `allocateMemory`
---@class AOBAlignmentType: integer Alignment mode accepted by AOB scan functions

---@type Address
process = 0

---@class StringList
---@field Count integer
---@field Text string
---@field String table<integer, string>
---@field Strings table<integer, string>
---@field add fun(self: StringList, value: string): integer
---@field clear fun(self: StringList)
---@field delete fun(self: StringList, index: integer)
---@field destroy fun(self: StringList)

---@class AutoAssemblerAllocation
---@field address Address
---@field size integer
---@field prefered Address?

---@class AutoAssemblerDisableInfo
---@field allocs table<string, AutoAssemblerAllocation>
---@field registeredsymbols string[]
---@field ccodesymbols any?
---@field exceptionlist Address[]
---@field symbols table<string, Address>

---@class RegisteredSymbol
---@field symbolname SymbolName
---@field address Address
---@field allocsize integer?
---@field processid integer?
---@field donotsave boolean?

---Resolves an address expression using the target or local symbol handler.
---@param expression AddressExpression|SymbolName
---@param localSymbolHandler boolean?
---@return Address
function getAddress(expression, localSymbolHandler) end

---@param expression AddressExpression|SymbolName
---@return Address
function getPointerAddress(expression) end

---Registers a user-defined symbol.
---@param symbol SymbolName
---@param address Address|AddressExpression
---@param doNotSave boolean?
function registerSymbol(symbol, address, doNotSave) end

---Removes a user-defined symbol.
---@param symbol SymbolName
function unregisterSymbol(symbol) end

---Returns symbols registered through Lua and auto assembler scripts.
---@return RegisteredSymbol[]
function enumRegisteredSymbols() end

---Removes all symbols registered through Lua and auto assembler scripts.
function deleteAllRegisteredSymbols() end

---Scans the target process for an array-of-bytes pattern.
---@overload fun(pattern: AOBPattern, protectionFlags?: string, alignmentType?: AOBAlignmentType, alignmentParam?: string): StringList
---@param ... integer Individual byte values; values outside the byte range act as wildcards
---@return StringList
function AOBScan(...) end

---Returns the first address matching an array-of-bytes pattern.
---@param pattern AOBPattern
---@param protectionFlags string?
---@param alignmentType AOBAlignmentType?
---@param alignmentParam string?
---@return Address?
function AOBScanUnique(pattern, protectionFlags, alignmentType, alignmentParam) end

---Returns the first pattern match in a module.
---@param moduleName string
---@param pattern AOBPattern
---@param protectionFlags string?
---@param alignmentType AOBAlignmentType?
---@param alignmentParam string?
---@return Address?
function AOBScanModuleUnique(moduleName, pattern, protectionFlags, alignmentType, alignmentParam) end

---Runs an auto assembler script or applies its disable section.
---@overload fun(text: string, disableInfo?: AutoAssemblerDisableInfo): boolean, AutoAssemblerDisableInfo, string[]?
---@param text string
---@param targetSelf boolean?
---@param disableInfo AutoAssemblerDisableInfo?
---@return boolean success
---@return AutoAssemblerDisableInfo disableInfo
---@return string[]? warnings
function autoAssemble(text, targetSelf, disableInfo) end

---Checks an auto assembler script for syntax errors without executing it.
---@param text string
---@param enable boolean?
---@param targetSelf boolean?
---@return boolean success
---@return string? errorMessage
function autoAssembleCheck(text, enable, targetSelf) end

---@return boolean
function target64Bit() end

---@return boolean
function targetIs64Bit() end

---@return boolean
function isAttached() end

---@return number
function getCEVersion() end

function beep() end
