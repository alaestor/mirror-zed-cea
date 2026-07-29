---@meta

---@class Structure
---@field Name string
---@field Size integer
---@field Count integer
---@field Internal boolean
---@field Element table<integer, StructureElement>
---@field getName fun(self: Structure): string
---@field setName fun(self: Structure, name: string)
---@field getElement fun(self: Structure, index: integer): StructureElement?
---@field getElementByOffset fun(self: Structure, offset: integer): StructureElement?
---@field getElementByOffsetExact fun(self: Structure, offset: integer): StructureElement?
---@field addElement fun(self: Structure): StructureElement
---@field autoGuess fun(self: Structure, baseAddress: Address|AddressExpression, offset: integer, size: integer)
---@field fillFromDotNetAddress fun(self: Structure, address: Address|AddressExpression, changeName: boolean)
---@field beginUpdate fun(self: Structure)
---@field endUpdate fun(self: Structure)
---@field addToGlobalStructureList fun(self: Structure)
---@field removeFromGlobalStructureList fun(self: Structure)
---@field clone fun(self: Structure, newName: string): Structure

---@class StructureElement
---@field Owner Structure
---@field Offset integer
---@field Name string
---@field Vartype integer
---@field BitStart integer
---@field BitSize integer
---@field CustomType any?
---@field CustomTypeName string
---@field DisplayMethod string
---@field ChildStruct Structure?
---@field ChildStructStart integer
---@field ChildClassName string
---@field Bytesize integer
---@field BackgroundColor integer
---@field OnCreateChild fun(sender: StructureElement, address?: Address): Structure?, boolean?
---@field getOwnerStructure fun(self: StructureElement): Structure
---@field getOffset fun(self: StructureElement): integer
---@field setOffset fun(self: StructureElement, offset: integer)
---@field getName fun(self: StructureElement): string
---@field setName fun(self: StructureElement, name: string)
---@field getVartype fun(self: StructureElement): integer
---@field setVartype fun(self: StructureElement, vartype: integer)
---@field getValue fun(self: StructureElement, address: Address|AddressExpression): any
---@field setValue fun(self: StructureElement, address: Address|AddressExpression, value: any)
---@field getValueFromBase fun(self: StructureElement, baseAddress: Address|AddressExpression): any
---@field setValueFromBase fun(self: StructureElement, baseAddress: Address|AddressExpression, value: any)
---@field getChildStruct fun(self: StructureElement): Structure?
---@field setChildStruct fun(self: StructureElement, structure: Structure?)
---@field getChildStructStart fun(self: StructureElement): integer
---@field setChildStructStart fun(self: StructureElement, offset: integer)
---@field getBytesize fun(self: StructureElement): integer
---@field setBytesize fun(self: StructureElement, size: integer)

---Returns the number of structures in Cheat Engine's global structure list.
---@return integer
function getStructureCount() end

---Returns a global or internal structure by index or case-sensitive name.
---@param indexOrName integer|string
---@return Structure?
function getStructure(indexOrName) end

---Creates an empty structure that is not yet in the global structure list.
---@param name string
---@return Structure
function createStructure(name) end

---Creates a structure from loaded type information.
---@param name string
---@return Structure?
function createStructureFromName(name) end
