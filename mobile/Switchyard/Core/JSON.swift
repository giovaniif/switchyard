import Foundation

/// A JSON value. yardd's messages are read through this rather than strict
/// Codable types, so a field a newer or older yardd adds or drops never
/// breaks decoding (the web UI is just as lenient).
enum JSON: Equatable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSON])
    case object([String: JSON])

    static func parse(_ data: Data) -> JSON? {
        guard let any = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) else { return nil }
        return JSON(any: any)
    }

    static func parse(_ text: String) -> JSON? { parse(Data(text.utf8)) }

    init(any: Any) {
        switch any {
        case let v as NSNumber:
            // JSONSerialization gives booleans as NSNumber too; tell them apart by type.
            if CFGetTypeID(v) == CFBooleanGetTypeID() { self = .bool(v.boolValue) } else { self = .number(v.doubleValue) }
        case let v as String: self = .string(v)
        case let v as [Any]: self = .array(v.map(JSON.init(any:)))
        case let v as [String: Any]: self = .object(v.mapValues(JSON.init(any:)))
        default: self = .null
        }
    }

    var any: Any {
        switch self {
        case .null: return NSNull()
        case .bool(let v): return v
        case .number(let v): return v.rounded() == v && abs(v) < 1e15 ? Int(v) as Any : v as Any
        case .string(let v): return v
        case .array(let v): return v.map(\.any)
        case .object(let v): return v.mapValues(\.any)
        }
    }

    func encoded() -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: any, options: [.fragmentsAllowed]) else { return "null" }
        return String(decoding: data, as: UTF8.self)
    }

    subscript(key: String) -> JSON {
        if case .object(let o) = self { return o[key] ?? .null }
        return .null
    }

    subscript(index: Int) -> JSON {
        if case .array(let a) = self, a.indices.contains(index) { return a[index] }
        return .null
    }

    var string: String? { if case .string(let v) = self { return v }; return nil }
    var double: Double? { if case .number(let v) = self { return v }; return nil }
    var int: Int? { double.map { Int($0) } }
    var bool: Bool? { if case .bool(let v) = self { return v }; return nil }
    var array: [JSON]? { if case .array(let v) = self { return v }; return nil }
    var object: [String: JSON]? { if case .object(let v) = self { return v }; return nil }
    var isNull: Bool { self == .null }

    /// The string, or "" when missing: most text fields are optional on the wire.
    var text: String { string ?? "" }
    var list: [JSON] { array ?? [] }
}

extension JSON: ExpressibleByStringLiteral, ExpressibleByBooleanLiteral, ExpressibleByIntegerLiteral,
    ExpressibleByDictionaryLiteral, ExpressibleByArrayLiteral, ExpressibleByNilLiteral, ExpressibleByFloatLiteral {
    init(stringLiteral value: String) { self = .string(value) }
    init(booleanLiteral value: Bool) { self = .bool(value) }
    init(integerLiteral value: Int) { self = .number(Double(value)) }
    init(floatLiteral value: Double) { self = .number(value) }
    init(dictionaryLiteral elements: (String, JSON)...) { self = .object(Dictionary(elements, uniquingKeysWith: { $1 })) }
    init(arrayLiteral elements: JSON...) { self = .array(elements) }
    init(nilLiteral: ()) { self = .null }
}

extension JSON {
    init(_ value: String?) { self = value.map(JSON.string) ?? .null }
    init(_ values: [String]) { self = .array(values.map(JSON.string)) }
}
