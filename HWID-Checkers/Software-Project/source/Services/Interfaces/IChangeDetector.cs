using System.Collections.Generic;
using HWIDChecker.Services.Models;

namespace HWIDChecker.Services.Interfaces
{
    public interface IChangeDetector
    {
        List<ComparisonResult> DetectChanges(List<(ComponentIdentifier Base, ComponentIdentifier Target)> matches);
    }
}