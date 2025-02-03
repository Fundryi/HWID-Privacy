using System.Collections.Generic;
using HWIDChecker.Services.Models;

namespace HWIDChecker.Services.Interfaces
{
    public interface IComponentMatcher
    {
        List<(ComponentIdentifier Base, ComponentIdentifier Target)> MatchComponents(
            List<ComponentIdentifier> baseComponents,
            List<ComponentIdentifier> targetComponents);
    }
}