using System.Collections.Generic;
using HWIDChecker.Services.Models;

namespace HWIDChecker.Services.Interfaces
{
    public interface IComponentParser
    {
        List<ComponentIdentifier> Parse(string configurationData);
    }
}