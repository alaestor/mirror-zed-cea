---@meta

---@param size integer
---@param baseAddress Address?
---@param protection MemoryProtection?
---@return Address?
function allocateMemory(size, baseAddress, protection) end

---@param address Address
---@param size integer?
---@return boolean
function deAlloc(address, size) end

---@param address Address|AddressExpression
---@param signed boolean?
---@return integer?
function readByte(address, signed) end

---@param address Address|AddressExpression
---@param signed boolean?
---@return integer?
function readSmallInteger(address, signed) end

---@param address Address|AddressExpression
---@param signed boolean?
---@return integer?
function readInteger(address, signed) end

---@param address Address|AddressExpression
---@return integer?
function readQword(address) end

---@param address Address|AddressExpression
---@return Pointer?
function readPointer(address) end

---@param address Address|AddressExpression
---@return number?
function readFloat(address) end

---@param address Address|AddressExpression
---@return number?
function readDouble(address) end

---@param address Address|AddressExpression
---@param maximumLength integer
---@param wideCharacter boolean?
---@return string?
function readString(address, maximumLength, wideCharacter) end

---@overload fun(address: Address|AddressExpression, byteCount: integer, returnAsTable: true): integer[]
---@overload fun(address: Address|AddressExpression, byteCount: integer, returnAsTable?: false): integer, ...
---@param address Address|AddressExpression
---@param byteCount integer
---@param returnAsTable boolean?
---@return integer|integer[]|nil
function readBytes(address, byteCount, returnAsTable) end

---@param address Address|AddressExpression
---@param size integer
---@return string?
function readMemory(address, size) end

---@overload fun(address: Address|AddressExpression, bytes: integer[]): boolean
---@param address Address|AddressExpression
---@param ... integer
---@return boolean
function writeBytes(address, ...) end

---@param address Address|AddressExpression
---@param value integer
---@return boolean
function writeByte(address, value) end

---@param address Address|AddressExpression
---@param value integer
---@return boolean
function writeSmallInteger(address, value) end

---@param address Address|AddressExpression
---@param value integer
---@return boolean
function writeInteger(address, value) end

---@param address Address|AddressExpression
---@param value integer
---@return boolean
function writeQword(address, value) end

---@param address Address|AddressExpression
---@param value Pointer
---@return boolean
function writePointer(address, value) end

---@param address Address|AddressExpression
---@param value number
---@return boolean
function writeFloat(address, value) end

---@param address Address|AddressExpression
---@param value number
---@return boolean
function writeDouble(address, value) end

---@param address Address|AddressExpression
---@param value string
---@param wideCharacter boolean?
---@return boolean
function writeString(address, value, wideCharacter) end

---@param address Address|AddressExpression
---@param value string
---@return boolean
function writeMemory(address, value) end
