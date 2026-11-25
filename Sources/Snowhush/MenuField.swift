import AppKit

/// A menu row holding a text field, for pasting into without leaving the menu. Return hands the text to `submit`.
final class MenuField: NSView {
    private let field = EditingField()
    private let submit: (String) -> Void

    init(placeholder: String, submit: @escaping (String) -> Void) {
        self.submit = submit
        super.init(frame: NSRect(x: 0, y: 0, width: 220, height: MenuToggle.rowHeight + 6))
        autoresizingMask = .width
        field.placeholderString = placeholder
        field.font = .menuFont(ofSize: 0)
        field.bezelStyle = .roundedBezel
        field.focusRingType = .none
        field.lineBreakMode = .byTruncatingMiddle
        field.cell?.usesSingleLineMode = true
        field.target = self
        field.action = #selector(returned)
        addSubview(field)
    }

    required init?(coder: NSCoder) { nil }

    override func layout() {
        super.layout()
        let inset = MenuToggle.plainTitleX - 4
        field.frame = NSRect(x: inset, y: 3, width: bounds.width - inset * 2, height: bounds.height - 6)
    }

    /// The menu's window only takes typing once the field is clicked into; a click anywhere on the row will do.
    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(field)
    }

    func clear() {
        field.stringValue = ""
    }

    @objc private func returned() {
        let text = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        submit(text)
    }
}

/// A menu bar app has no Edit menu to carry ⌘V and the rest to a field, so the field takes them itself.
private final class EditingField: NSTextField {
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command, let editor = currentEditor() else {
            return super.performKeyEquivalent(with: event)
        }
        switch event.charactersIgnoringModifiers {
        case "v": editor.paste(nil)
        case "c": editor.copy(nil)
        case "x": editor.cut(nil)
        case "a": editor.selectAll(nil)
        case "z": editor.undoManager?.undo()
        default: return super.performKeyEquivalent(with: event)
        }
        return true
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        super.mouseDown(with: event)
    }
}
