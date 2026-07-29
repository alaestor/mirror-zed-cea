---@meta

---@class MessageDialogType: integer
---@class MessageDialogButton: integer
---@class ModalResult: integer
---@class MainForm
---@field OnProcessOpened fun()

---@param text any
function showMessage(text) end

---@param caption string
---@param prompt string
---@param initialString string
---@return string?
function inputQuery(caption, prompt, initialString) end

---@overload fun(text: string): ModalResult
---@overload fun(title: string, text: string, dialogType: MessageDialogType, ...: MessageDialogButton): ModalResult
---@param text string
---@param dialogType MessageDialogType
---@param ... MessageDialogButton
---@return ModalResult
function messageDialog(text, dialogType, ...) end

---@type MessageDialogType
mtWarning = 0
---@type MessageDialogType
mtError = 1
---@type MessageDialogType
mtInformation = 2
---@type MessageDialogType
mtConfirmation = 3

---@type MessageDialogButton
mbYes = 0
---@type MessageDialogButton
mbNo = 1
---@type MessageDialogButton
mbOK = 2
---@type MessageDialogButton
mbCancel = 3

---@type ModalResult
mrNone = 0
---@type ModalResult
mrOk = 1
---@type ModalResult
mrCancel = 2
---@type ModalResult
mrAbort = 3
---@type ModalResult
mrRetry = 4
---@type ModalResult
mrIgnore = 5
---@type ModalResult
mrYes = 6
---@type ModalResult
mrNo = 7
