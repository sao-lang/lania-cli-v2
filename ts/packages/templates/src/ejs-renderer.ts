/**
 * EJS 渲染封装，统一 include 路径与模板上下文。
 *
 * 主要导出：EjsRenderer、ejsRenderer。
 *
 * 说明：
 * - 这里使用 `async: true`，允许模板中使用 async include / await（便于后续扩展）。
 * - EJS 模板本质是可执行代码：模板资产应当来自本地受信任目录（本包内置模板或项目本地模板）。
 */
import ejs from 'ejs';

export class EjsRenderer {
  /**
   * 渲染一段字符串模板（较少使用，主要用于测试或临时模板）。
   */
  async renderString(template: string, data: Record<string, unknown>) {
    return ejs.render(template, data, { async: true });
  }

  /**
   * 渲染一个文件模板。
   * - filePath 通常指向模板目录下的 `ejs/*.ejs` 资产，或 `add-templates/assets/*.ejs`
   * - data 为渲染上下文（locals），会被模板直接访问
   */
  async renderFile(filePath: string, data: Record<string, unknown>) {
    return ejs.renderFile(filePath, data, { async: true });
  }
}

export const ejsRenderer = new EjsRenderer();
