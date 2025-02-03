# Unified Hardware Comparison System Architecture

## Core Objectives

- Reliable component matching using unique identifiers
- High-performance comparison engine
- Intuitive file selection interface
- Comprehensive change tracking
- Enterprise-grade error handling

---

## Component Identification System

### Cross-Component Strategies (CompareUpdate.md)

```typescript
interface ComponentIdentifier {
  type: string;
  uniqueKey: string;
  properties: Map<string, string>;
}
```

### C# Implementation Details (Implementation.md)

```csharp
public class ComponentIdentifier {
    public string Type { get; set; }
    public string UniqueKey { get; set; }
    public Dictionary<string, string> Properties { get; set; }
}
```

### Hardware-Specific Identifiers

| Component   | Primary Key                      | Fallback Strategy              |
| ----------- | -------------------------------- | ------------------------------ |
| **Storage** | `{PhysicalDrive}-{SerialNumber}` | Device path + capacity         |
| **Memory**  | `{DeviceLocator}-{SerialNumber}` | Type + capacity + speed + slot |
| **GPU**     | `{UUID}`                         | Device ID + Vendor ID          |
| **Network** | `{MACAddress}`                   | Adapter type + driver version  |

---

## Comparison Engine Architecture

### Service Layer (Implementation.md)

```csharp
public class CompareService {
    public async Task<List<ComparisonResult>> CompareConfigurations(
        string baseConfig,
        string targetConfig) {
        // Core comparison logic
    }
}
```

### Processing Pipeline (CompareUpdate.md)

1. Component Classification
2. Change Detection
3. Data Verification
4. Result Compilation

---

## Unified UI Requirements

### File Handling

- **Selection Interface**:
  ```csharp
  var files = Directory.GetFiles(".", "HWID-EXPORT-*.txt")
      .OrderByDescending(f => ParseExportDate(f));
  ```
- **Auto-Refresh**: Detect new exports during comparison sessions

### Result Presentation

- Color-coded change types (Added/Modified/Removed)
- Collapsible technical details section
- Export options (HTML/CSV)

---

## Performance Optimization

### Parallel Processing

```csharp
public async Task<List<ComparisonResult>> CompareParallel() {
    await Task.WhenAll(/* parsing tasks */);
    return await Task.Run(() => DetectChanges());
}
```

### Caching Strategy

```csharp
public class ComparisonCache {
    public async Task<ComparisonResult> GetOrCreateComparison(
        string cacheKey,
        Func<Task<ComparisonResult>> factory) {
        return await _cache.GetOrCreateAsync(cacheKey, factory);
    }
}
```

---

## Error Handling Framework

### Unified Exception Model

```csharp
public class ComparisonException : Exception {
    public string ComponentType { get; }
    public string ErrorCode { get; }

    // Constructor handles both C# and TypeScript error scenarios
}
```

### Validation Layers

1. File format verification
2. Component identifier sanity checks
3. Change plausibility analysis

---

## Implementation Roadmap

1. **Core Infrastructure**

   - Merge component identification systems
   - Implement unified comparison service

2. **UI Modernization**

   - Build file selector with date sorting
   - Develop new result visualization

3. **Optimization Phase**

   - Add parallel processing
   - Implement caching layer

4. **Validation & Testing**
   - Unit tests for component matching
   - Integration tests for full workflow

---

## Key Metrics

- Accuracy: >99.9% correct change detection
- Performance: <2s for typical comparisons
- Reliability: <0.1% false positives
