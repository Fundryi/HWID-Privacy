using System.Collections.Generic;

namespace HWIDChecker.Services.Models
{
    public class ComponentIdentifier
    {
        public string Type { get; set; }
        public string UniqueKey { get; set; }
        public Dictionary<string, string> Properties { get; set; } = new Dictionary<string, string>();

        public ComponentIdentifier(string type, string uniqueKey)
        {
            Type = type;
            UniqueKey = uniqueKey;
        }

        public void AddProperty(string key, string value)
        {
            Properties[key] = value;
        }

        public string GetProperty(string key)
        {
            return Properties.TryGetValue(key, out var value) ? value : string.Empty;
        }
    }

    public class ComparisonResult
    {
        public string ComponentType { get; set; }
        public ChangeType ChangeType { get; set; }
        public Dictionary<string, (string OldValue, string NewValue)> Changes { get; set; } =
            new Dictionary<string, (string OldValue, string NewValue)>();

        public ComparisonResult(string componentType, ChangeType changeType)
        {
            ComponentType = componentType;
            ChangeType = changeType;
        }

        public void AddChange(string property, string oldValue, string newValue)
        {
            Changes[property] = (oldValue, newValue);
        }
    }

    public enum ChangeType
    {
        Added,
        Removed,
        Modified,
        Unchanged
    }
}