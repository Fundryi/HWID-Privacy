using System.Collections.Generic;

namespace HWIDChecker.Services.Interfaces
{
    public interface IComponentIdentifierStrategy
    {
        string[] GetComparisonProperties();
        string GetIdentifier(Dictionary<string, string> properties);
        string[] GetFallbackIdentifiers(Dictionary<string, string> properties);
    }
}