// Toolchain fixture only; not emitted by the Rust compiler.
[[vk::binding(0, 0)]] StructuredBuffer<float> inputA;
[[vk::binding(1, 0)]] StructuredBuffer<float> inputB;
[[vk::binding(2, 0)]] RWStructuredBuffer<float> outputC;
[[vk::binding(3, 0)]] StructuredBuffer<uint> elementCount;

[shader("compute")]
[numthreads(64, 1, 1)]
void computeMain(uint3 gid : SV_DispatchThreadID)
{
    uint i = gid.x;
    if (i < elementCount[0])
        outputC[i] = inputA[i] + inputB[i];
}
