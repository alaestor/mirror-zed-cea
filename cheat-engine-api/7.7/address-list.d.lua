---@meta

---@class MemoryRecordId: integer Stable ID assigned to a memory record
---@class VirtualType: integer Numeric `vt*` variable-type constant

---@type VirtualType
vtByte = 0
---@type VirtualType
vtWord = 1
---@type VirtualType
vtDword = 2
---@type VirtualType
vtQword = 3
---@type VirtualType
vtSingle = 4
---@type VirtualType
vtDouble = 5
---@type VirtualType
vtString = 6
---@type VirtualType
vtUnicodeString = 7
---@type VirtualType
vtWideString = 7
---@type VirtualType
vtByteArray = 8
---@type VirtualType
vtBinary = 9
---@type VirtualType
vtAll = 10
---@type VirtualType
vtAutoAssembler = 11
---@type VirtualType
vtPointer = 12
---@type VirtualType
vtCustom = 13
---@type VirtualType
vtGrouped = 14

---@class MemoryRecord
---@field ID MemoryRecordId
---@field Index integer
---@field Description string
---@field Address string
---@field AddressString string
---@field CurrentAddress Address
---@field OffsetCount integer
---@field Offset table<integer, integer>
---@field OffsetText table<integer, string>
---@field VarType string
---@field Type VirtualType
---@field String { Size: number, Unicode: boolean, Codepage: boolean }
---@field Binary { Startbit: number, Size: number }
---@field Aob { Size: number }
---@field CustomTypeName string
---@field Script string
---@field Value string
---@field NumericalValue number?
---@field Active boolean
---@field Color integer
---@field ShowAsHex boolean
---@field ShowAsSigned boolean
---@field AllowIncrease boolean
---@field AllowDecrease boolean
---@field Collapsed boolean
---@field Async boolean
---@field IsGroupHeader boolean
---@field IsAddressGroupHeader boolean
---@field DontSave boolean
---@field Options string
---@field DropDownList StringList
---@field DropDownLinkedMemrec string
---@field DropDownReadOnly boolean
---@field DropDownDescriptionOnly boolean
---@field DisplayAsDropDownListItem boolean
---@field Parent MemoryRecord?
---@field Child table<integer, MemoryRecord>
---@field Count integer
---@field appendToEntry fun(self: MemoryRecord, parent: MemoryRecord)
---@field destroy fun(self: MemoryRecord)
---@field disableWithoutExecute fun(self: MemoryRecord)
---@field getDescription fun(self: MemoryRecord): string
---@field setDescription fun(self: MemoryRecord, description: string)
---@field getAddress fun(self: MemoryRecord): string, integer[]?
---@field setAddress fun(self: MemoryRecord, address: string, offsets: integer[]?)
---@field getOffsetCount fun(self: MemoryRecord): integer
---@field setOffsetCount fun(self: MemoryRecord, count: integer)
---@field getOffset fun(self: MemoryRecord, index: integer): integer
---@field setOffset fun(self: MemoryRecord, index: integer, offset: integer)
---@field getCurrentAddress fun(self: MemoryRecord): Address
---@field beginEdit fun(self: MemoryRecord)
---@field endEdit fun(self: MemoryRecord)

---@class AddressList
---@field Count integer
---@field SelCount integer
---@field SelectedRecord MemoryRecord?
---@field MemoryRecord table<integer, MemoryRecord>
---@field createMemoryRecord fun(self: AddressList): MemoryRecord
---@field getCount fun(self: AddressList): integer
---@field getMemoryRecord fun(self: AddressList, index: integer): MemoryRecord?
---@field getMemoryRecordByDescription fun(self: AddressList, description: string): MemoryRecord?
---@field getMemoryRecordsWithDescription fun(self: AddressList, description: string): MemoryRecord[]
---@field getMemoryRecordByID fun(self: AddressList, id: MemoryRecordId): MemoryRecord?
---@field getSelectedRecords fun(self: AddressList): MemoryRecord[]
---@field getSelectedRecord fun(self: AddressList): MemoryRecord?
---@field setSelectedRecord fun(self: AddressList, record: MemoryRecord?)
---@field disableAllWithoutExecute fun(self: AddressList)
---@field rebuildDescriptionCache fun(self: AddressList)

---@return AddressList
function getAddressList() end
