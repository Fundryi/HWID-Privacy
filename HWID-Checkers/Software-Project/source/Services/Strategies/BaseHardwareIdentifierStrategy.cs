using System.Collections.Generic;
using System.Linq;
using HWIDChecker.Services.Interfaces;

namespace HWIDChecker.Services.Strategies
{
    public abstract class BaseHardwareIdentifierStrategy : IComponentIdentifierStrategy
    {
        public abstract string[] GetComparisonProperties();

        public virtual string GetIdentifier(Dictionary<string, string> properties)
        {
            var mainIdentifierProperty = GetComparisonProperties().FirstOrDefault();
            return mainIdentifierProperty != null && properties.TryGetValue(mainIdentifierProperty, out var value)
                ? value
                : null;
        }

        public virtual string[] GetFallbackIdentifiers(Dictionary<string, string> properties)
        {
            var idProperties = GetComparisonProperties();
            var fallbacks = new List<string>();

            foreach (var prop in idProperties.Skip(1)) // Skip primary identifier
            {
                if (properties.TryGetValue(prop, out var value))
                {
                    fallbacks.Add(value);
                }
            }

            return fallbacks.ToArray();
        }
    }
}