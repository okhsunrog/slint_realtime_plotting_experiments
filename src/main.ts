/// <reference types="@webgpu/types" />
import { invoke } from '@tauri-apps/api/core';
import './styles.css';

const root = document.querySelector<HTMLDivElement>('#app')!;
root.innerHTML = `
  <h1>CEF / Wayland / WebGPU</h1>
  <p>Native embedding, input and Vulkan diagnostics</p>
  <div class="controls">
    <button id="run">Run GPU test</button><button id="gpu">Open chrome://gpu</button>
    <button id="version">Open chrome://version</button><button id="save">Save report</button>
  </div>
  <canvas width="640" height="240"></canvas>
  <fieldset><legend>Input and popup checks</legend>
    <label>Typing / clipboard / IME <input placeholder="Русский текст, emoji, Ctrl+V" /></label>
    <label>Native select <select><option>First item</option><option>Second item</option><option>Third item</option></select></label>
    <button id="alert">JavaScript dialog</button>
    <p>Resize, maximize and move between displays with different scales. Check the bottom and right edges.</p>
  </fieldset>
  <pre id="result">Preparing diagnostics…</pre>`;
const output = document.querySelector<HTMLPreElement>('#result')!;
let report: Record<string, unknown> = {};
let device: GPUDevice | undefined;
let frame = 0;
function show() { output.textContent = JSON.stringify(report, null, 2); }
async function test() {
  cancelAnimationFrame(frame);
  device?.destroy();
  report = { time: new Date().toISOString(), secureContext: isSecureContext,
    userAgent: navigator.userAgent, devicePixelRatio,
    viewport: [innerWidth, innerHeight], runtime: await invoke('runtime_info') };
  try {
    if (!navigator.gpu) throw new Error('navigator.gpu is unavailable');
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
    if (!adapter) throw new Error('No WebGPU adapter');
    const info = adapter.info;
    report.adapter = { vendor: info.vendor, architecture: info.architecture,
      device: info.device, description: info.description, isFallbackAdapter: info.isFallbackAdapter };
    device = await adapter.requestDevice();
    const gpu = device;
    gpu.addEventListener('uncapturederror', event => { report.uncapturedError = event.error.message; show(); });
    void gpu.lost.then(lost => { if (lost.reason !== 'destroyed') { report.deviceLost = lost.message; show(); } });
    gpu.pushErrorScope('validation');
    const count = 1024;
    const data = gpu.createBuffer({ size: count * 4, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
    const read = gpu.createBuffer({ size: count * 4, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const shader = gpu.createShaderModule({ code: `
      @group(0) @binding(0) var<storage, read_write> values: array<u32>;
      @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id: vec3u) {
        if (id.x < 1024u) { values[id.x] = id.x * id.x + 7u; }
      }` });
    const pipeline = await gpu.createComputePipelineAsync({ layout: 'auto', compute: { module: shader } });
    const bindings = gpu.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries: [{ binding: 0, resource: { buffer: data } }] });
    const encoder = gpu.createCommandEncoder();
    const pass = encoder.beginComputePass();
    pass.setPipeline(pipeline); pass.setBindGroup(0, bindings); pass.dispatchWorkgroups(count / 64); pass.end();
    encoder.copyBufferToBuffer(data, 0, read, 0, count * 4);
    gpu.queue.submit([encoder.finish()]);
    await read.mapAsync(GPUMapMode.READ);
    const values = new Uint32Array(read.getMappedRange());
    for (let i = 0; i < count; i++) if (values[i] !== i * i + 7) throw new Error(`Compute mismatch at ${i}`);
    report.compute = { result: 'PASS', checkedValues: count, lastValue: values[count - 1] };
    read.unmap(); read.destroy(); data.destroy();
    const error = await gpu.popErrorScope();
    if (error) throw new Error(error.message);
    const canvas = document.querySelector('canvas')!;
    const context = canvas.getContext('webgpu');
    if (!context) throw new Error('WebGPU canvas context unavailable');
    context.configure({ device: gpu, format: navigator.gpu.getPreferredCanvasFormat(), alphaMode: 'opaque' });
    function render(time: number) {
      const width = Math.max(1, Math.round(canvas.clientWidth * devicePixelRatio));
      const height = Math.max(1, Math.round(canvas.clientHeight * devicePixelRatio));
      if (canvas.width !== width) canvas.width = width;
      if (canvas.height !== height) canvas.height = height;
      const command = gpu.createCommandEncoder();
      const renderPass = command.beginRenderPass({ colorAttachments: [{
        view: context!.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store',
        clearValue: { r: .07, g: .2 + Math.sin(time / 1000) * .12, b: .45, a: 1 },
      }] });
      renderPass.end(); gpu.queue.submit([command.finish()]);
      frame = requestAnimationFrame(render);
    }
    frame = requestAnimationFrame(render);
    report.presentation = 'Animated WebGPU canvas started; visually verify resize and edges';
    report.backendVerification = 'Open chrome://gpu: confirm Vulkan and hardware WebGPU. JS adapter information alone does not prove Vulkan.';
    report.hardware = info.isFallbackAdapter ? 'FAIL: fallback adapter' : 'Adapter reports non-fallback; verify driver/backend in chrome://gpu';
  } catch (error) { report.error = String(error); }
  try { report.savedTo = await invoke('save_report', { report }); }
  catch (error) { report.saveError = String(error); }
  show();
}
document.querySelector('#run')!.addEventListener('click', () => { void test(); });
document.querySelector('#gpu')!.addEventListener('click', () => { void invoke('open_diagnostic', { page: 'gpu' }); });
document.querySelector('#version')!.addEventListener('click', () => { void invoke('open_diagnostic', { page: 'version' }); });
document.querySelector('#alert')!.addEventListener('click', () => alert('Check keyboard focus after closing this dialog.'));
document.querySelector('#save')!.addEventListener('click', async () => {
  try { report.savedTo = await invoke('save_report', { report }); } catch (error) { report.saveError = String(error); }
  show();
});
window.addEventListener('resize', () => { report.viewport = [innerWidth, innerHeight]; report.devicePixelRatio = devicePixelRatio; show(); });
void test();
