/**
 * `tools.text`：面向终端文本样式的小型工具集。
 *
 * 这里主要服务于 CLI/日志输出增强：
 * - 直接渲染带 ANSI 样式的字符串
 * - 通过链式 handle 叠加 bold/underline/color 等效果
 * - 提供 RGB/HSL 到 ANSI 颜色的转换
 */
export interface TextStyleHandle {
  config: (options: TextRenderOptions) => TextStyleHandle;
  bold: () => TextStyleHandle;
  italic: () => TextStyleHandle;
  underline: () => TextStyleHandle;
  overline: () => TextStyleHandle;
  inverse: () => TextStyleHandle;
  strikethrough: () => TextStyleHandle;
  hidden: () => TextStyleHandle;
  visible: () => TextStyleHandle;
  render: () => string;
}

export interface TextColorValue {
  r: number;
  g: number;
  b: number;
  toAnsiForeground: () => string;
  toAnsiBackground: () => string;
}

export interface TextRenderOptions {
  prefix?: string;
  suffix?: string;
  color?: TextColorValue;
  backgroundColor?: TextColorValue;
}

export interface TextTools {
  style: (content: string, options?: TextRenderOptions) => TextStyleHandle;
  render: (content: string, options?: TextRenderOptions) => string;
  rgb: (r: number, g: number, b: number) => TextColorValue;
  hsl: (h: number, s: number, l: number) => TextColorValue;
}

export function createTextTools(): TextTools {
  // `style()` 返回一个可继续链式配置的 handle；`render()` 则是一次性快捷入口。
  const style = (content: string, options?: TextRenderOptions) => createTextStyleHandle(content, options);

  return {
    style,
    render: (content, options) => style(content, options).render(),
    rgb: (r, g, b) => createRgbColor(r, g, b),
    hsl: (h, s, l) => {
      const normalizedHue = normalizeHue(h);
      const normalizedSaturation = clampPercent(s) / 100;
      const normalizedLightness = clampPercent(l) / 100;
      const chroma = (1 - Math.abs(2 * normalizedLightness - 1)) * normalizedSaturation;
      const huePrime = normalizedHue / 60;
      const second = chroma * (1 - Math.abs((huePrime % 2) - 1));
      const match = normalizedLightness - chroma / 2;

      let red = 0;
      let green = 0;
      let blue = 0;
      if (huePrime >= 0 && huePrime < 1) {
        red = chroma;
        green = second;
      } else if (huePrime < 2) {
        red = second;
        green = chroma;
      } else if (huePrime < 3) {
        green = chroma;
        blue = second;
      } else if (huePrime < 4) {
        green = second;
        blue = chroma;
      } else if (huePrime < 5) {
        red = second;
        blue = chroma;
      } else {
        red = chroma;
        blue = second;
      }

      return createRgbColor((red + match) * 255, (green + match) * 255, (blue + match) * 255);
    },
  };
}

function createTextStyleHandle(
  content: string,
  initial?: TextRenderOptions,
): TextStyleHandle {
  // handle 内部是可变状态，但对外表现为链式 API，适合逐步叠加文本样式。
  let prefix = initial?.prefix ?? '';
  let suffix = initial?.suffix ?? '';
  let color = initial?.color;
  let backgroundColor = initial?.backgroundColor;
  const modifiers: string[] = [];

  const enableModifier = (name: string) => {
    if (!modifiers.includes(name)) {
      modifiers.push(name);
    }
    return api;
  };

  const disableModifier = (name: string) => {
    const index = modifiers.indexOf(name);
    if (index >= 0) {
      modifiers.splice(index, 1);
    }
    return api;
  };

  const api: TextStyleHandle = {
    config: (options) => {
      prefix = typeof options.prefix === 'string' ? options.prefix : prefix;
      suffix = typeof options.suffix === 'string' ? options.suffix : suffix;
      color = options.color ?? color;
      backgroundColor = options.backgroundColor ?? backgroundColor;
      return api;
    },
    bold: () => enableModifier('bold'),
    italic: () => enableModifier('italic'),
    underline: () => enableModifier('underline'),
    overline: () => enableModifier('overline'),
    inverse: () => enableModifier('inverse'),
    strikethrough: () => enableModifier('strikethrough'),
    hidden: () => enableModifier('hidden'),
    visible: () => disableModifier('hidden'),
    render: () =>
      renderStyledText(content, {
        prefix,
        suffix,
        color,
        backgroundColor,
        modifiers,
      }),
  };

  return api;
}

function renderStyledText(
  content: string,
  options: TextRenderOptions & { modifiers?: string[] },
): string {
  // 统一在结尾补 `\u001b[0m`，保证样式不会泄漏到后续终端输出。
  const openCodes = (options.modifiers ?? [])
    .map((modifier) => ansiOpenCodeForModifier(modifier))
    .filter((value): value is string => Boolean(value));
  if (options.color) {
    openCodes.push(options.color.toAnsiForeground());
  }
  if (options.backgroundColor) {
    openCodes.push(options.backgroundColor.toAnsiBackground());
  }
  if (openCodes.length === 0) {
    return `${options.prefix ?? ''}${content}${options.suffix ?? ''}`;
  }
  return `${options.prefix ?? ''}${openCodes.join('')}${content}\u001b[0m${options.suffix ?? ''}`;
}

function ansiOpenCodeForModifier(modifier: string): string | null {
  switch (modifier) {
    case 'bold':
      return '\u001b[1m';
    case 'italic':
      return '\u001b[3m';
    case 'underline':
      return '\u001b[4m';
    case 'inverse':
      return '\u001b[7m';
    case 'hidden':
      return '\u001b[8m';
    case 'strikethrough':
      return '\u001b[9m';
    case 'overline':
      return '\u001b[53m';
    default:
      return null;
  }
}

function clampByte(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(255, Math.round(value)));
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(100, value));
}

function normalizeHue(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return ((value % 360) + 360) % 360;
}

function createRgbColor(r: number, g: number, b: number): TextColorValue {
  // RGB 颜色值在创建时就先钳制到合法字节范围，避免后续重复防御。
  const red = clampByte(r);
  const green = clampByte(g);
  const blue = clampByte(b);
  return {
    r: red,
    g: green,
    b: blue,
    toAnsiForeground: () => `\u001b[38;2;${red};${green};${blue}m`,
    toAnsiBackground: () => `\u001b[48;2;${red};${green};${blue}m`,
  };
}
