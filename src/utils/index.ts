import { Colord, colord } from 'colord';

// 随机生成颜色
export function randomColor(): Colord {
  const hue = Math.random() * 360;
  const saturation = 50 + Math.random() * 50; // 50-100%
  const lightness = 40 + Math.random() * 40; // 40-80%
  return colord({ h: hue, s: saturation, l: lightness });
}
