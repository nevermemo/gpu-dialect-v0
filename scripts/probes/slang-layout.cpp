// Standalone T06 evidence probe, not a Rust runtime dependency or ABI expansion.
// In an x64 VS Developer Command Prompt at the repository root:
// mkdir target\t06-layout
// cl /nologo /std:c++17 /EHsc /W4 /WX /I C:\VulkanSDK\1.4.357.0\Include\slang scripts\probes\slang-layout.cpp /Fo:target\t06-layout\slang-layout.obj /Fe:target\t06-layout\slang-layout.exe /link /LIBPATH:C:\VulkanSDK\1.4.357.0\Lib slang-compiler.lib
// set PATH=C:\VulkanSDK\1.4.357.0\Bin;%PATH%
// target\t06-layout\slang-layout.exe scripts\probes\layout.slang
// Target settings intentionally use Slang defaults, without scalar-layout overrides.
#include <slang.h>
#include <slang-com-ptr.h>
#include <fstream>
#include <iostream>
#include <iterator>
#include <stdexcept>
#include <string>

using Slang::ComPtr;

static void diagnostics(slang::IBlob* blob)
{
    if (blob && blob->getBufferSize())
        std::cerr.write(static_cast<const char*>(blob->getBufferPointer()), blob->getBufferSize());
}

static void check(SlangResult result, const char* operation, slang::IBlob* blob = nullptr)
{
    diagnostics(blob);
    if (SLANG_FAILED(result)) throw std::runtime_error(operation);
}

static size_t known(size_t value, const char* what)
{
    if (value == SLANG_UNKNOWN_SIZE) throw std::runtime_error(std::string(what) + " is unknown");
    if (value == SLANG_UNBOUNDED_SIZE) throw std::runtime_error(std::string(what) + " is unbounded");
    return value;
}

static void describe(slang::TypeLayoutReflection* type, const std::string& path, unsigned depth = 0)
{
    if (!type || depth > 32) throw std::runtime_error("missing or excessively nested element layout");
    const auto size = known(type->getSize(), "size");
    const auto stride = known(type->getStride(), "stride");
    const auto alignment = type->getAlignment();
    if (size == 0 || stride == 0 || alignment <= 0)
        throw std::runtime_error("missing byte layout for " + path);
    std::cout << path << " size=" << size << " alignment=" << alignment << " stride=" << stride << '\n';
    for (unsigned i = 0; i < type->getFieldCount(); ++i)
    {
        auto field = type->getFieldByIndex(i);
        if (!field || !field->getName()) throw std::runtime_error("missing field");
        const auto child = path + "." + field->getName();
        std::cout << child << " offset=" << known(field->getOffset(), "field offset") << '\n';
        describe(field->getTypeLayout(), child, depth + 1);
    }
}

static void probe(slang::IGlobalSession* global, const std::string& source,
                  const char* sourcePath, SlangCompileTarget format, const char* label)
{
    slang::TargetDesc target;
    target.format = format;
    slang::SessionDesc desc;
    desc.targets = &target;
    desc.targetCount = 1;
    ComPtr<slang::ISession> session;
    check(global->createSession(desc, session.writeRef()), "createSession failed");
    ComPtr<slang::IBlob> diag;
    auto module = session->loadModuleFromSourceString("layout_probe", sourcePath, source.c_str(), diag.writeRef());
    diagnostics(diag);
    if (!module) throw std::runtime_error("loadModuleFromSourceString failed");
    ComPtr<slang::IEntryPoint> entry;
    check(module->findEntryPointByName("main", entry.writeRef()), "findEntryPointByName failed");
    slang::IComponentType* components[] = {module, entry};
    ComPtr<slang::IComponentType> composite;
    auto result = session->createCompositeComponentType(components, 2, composite.writeRef(), diag.writeRef());
    check(result, "createCompositeComponentType failed", diag);
    ComPtr<slang::IComponentType> linked;
    result = composite->link(linked.writeRef(), diag.writeRef());
    check(result, "link failed", diag);
    ComPtr<slang::IBlob> code;
    result = linked->getTargetCode(0, code.writeRef(), diag.writeRef());
    check(result, "target code generation failed", diag);
    if (!code || !code->getBufferSize()) throw std::runtime_error("empty target code");
    auto layout = linked->getLayout(0, diag.writeRef());
    diagnostics(diag);
    if (!layout || layout->getParameterCount() != 4) throw std::runtime_error("missing fixture resources");
    std::cout << "target=" << label << " code_bytes=" << code->getBufferSize() << '\n';
    for (unsigned i = 0; i < layout->getParameterCount(); ++i)
    {
        auto resource = layout->getParameterByIndex(i);
        if (!resource || !resource->getName() || !resource->getTypeLayout())
            throw std::runtime_error("missing resource layout");
        const auto category = slang::ParameterCategory::DescriptorTableSlot;
        bool foundCategory = false;
        for (unsigned j = 0; j < resource->getCategoryCount(); ++j)
            foundCategory |= resource->getCategoryByIndex(j) == category;
        if (!foundCategory) throw std::runtime_error("missing descriptor binding category");
        std::cout << "resource=" << resource->getName()
                  << " binding=" << known(resource->getOffset(category), "binding index")
                  << " space=" << known(resource->getBindingSpace(category), "binding space") << '\n';
        describe(resource->getTypeLayout()->getElementTypeLayout(), resource->getName());
    }
}

int main(int argc, char** argv)
{
    try
    {
        if (argc != 2) throw std::runtime_error("usage: slang-layout <layout.slang>");
        std::ifstream stream(argv[1], std::ios::binary);
        if (!stream) throw std::runtime_error("cannot open fixture");
        const std::string source((std::istreambuf_iterator<char>(stream)), {});
        if (stream.bad() || source.empty()) throw std::runtime_error("cannot read fixture");
        ComPtr<slang::IGlobalSession> global;
        check(slang::createGlobalSession(global.writeRef()), "createGlobalSession failed");
        std::cout << "slang=" << global->getBuildTagString() << '\n';
        probe(global, source, argv[1], SLANG_WGSL, "wgsl");
        probe(global, source, argv[1], SLANG_SPIRV, "spirv");
        return 0;
    }
    catch (const std::exception& error)
    {
        std::cerr << "probe failed: " << error.what() << '\n';
        return 1;
    }
}
