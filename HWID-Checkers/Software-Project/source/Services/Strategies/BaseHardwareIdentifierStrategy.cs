using System.Collections.Generic;
using System.Linq;
using HWIDChecker.Services.Interfaces;

namespace HWIDChecker.Services.Strategies
{
    public abstract class BaseHardwareIdentifierStrategy : IComponentIdentifierStrategy
    {
        public abstract string[] GetComparisonProperties();

        public string GetIdentifier(Dictionary<string, string> properties)
        {
            return GetPrimaryKey(properties);
        }

        public string[] GetFallbackIdentifiers(Dictionary<string, string> properties)
        {
            var fallbackKey = GetFallbackKey(properties);
            return fallbackKey != null ? new[] { fallbackKey } : new string[0];
        }

        protected virtual string GetPrimaryKey(Dictionary<string, string> properties)
        {
            var primaryKeyParts = GetPrimaryKeyComponents();
            if (primaryKeyParts == null || !primaryKeyParts.Any())
            {
                return null;
            }

            var keyValues = new List<string>();
            foreach (var part in primaryKeyParts)
            {
                if (properties.TryGetValue(part, out var value))
                {
                    keyValues.Add(value);
                }
                else
                {
                    return null; // If any primary key component is missing, return null
                }
            }

            return string.Join("-", keyValues);
        }

        protected virtual string GetFallbackKey(Dictionary<string, string> properties)
        {
            var fallbackParts = GetFallbackKeyComponents();
            if (fallbackParts == null || !fallbackParts.Any())
            {
                return null;
            }

            var keyValues = new List<string>();
            foreach (var part in fallbackParts)
            {
                if (properties.TryGetValue(part, out var value))
                {
                    keyValues.Add(value);
                }
            }

            return keyValues.Any() ? string.Join("-", keyValues) : null;
        }

        protected abstract string[] GetPrimaryKeyComponents();
        protected abstract string[] GetFallbackKeyComponents();
    }
}