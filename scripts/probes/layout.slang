// Hand-authored investigation fixture, not newly supported Rust GPU syntax.
struct Inner { float value; uint tag; };
struct Pod { Inner inner; int delta; };
// Contrasts the existing four-byte-aligned, padding-free host contract.
struct Padded { float head; float3 direction; float tail; };

[[vk::binding(0, 0)]] StructuredBuffer<float> scalars;
[[vk::binding(1, 0)]] StructuredBuffer<Pod> pods;
[[vk::binding(2, 0)]] StructuredBuffer<Padded> padded;
[[vk::binding(3, 0)]] RWStructuredBuffer<float> output;

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    uint count, stride;
    output.GetDimensions(count, stride);
    if (id.x < count)
        output[id.x] = scalars[id.x] + pods[id.x].inner.value
            + float(pods[id.x].inner.tag) + float(pods[id.x].delta)
            + padded[id.x].head + padded[id.x].direction.x + padded[id.x].tail;
}
