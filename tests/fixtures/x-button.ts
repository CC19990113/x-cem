/**
 * `<x-button>`：按钮组件。
 *
 * 原生 Custom Element，内部渲染真 `<button>`，事件跨 Shadow DOM 冒泡。
 *
 * @attr {('default'|'primary'|'danger'|'ghost')} [variant='default'] - 视觉变体
 * @attr {('sm'|'md'|'lg')} [size='md'] - 尺寸
 * @attr {boolean} [disabled=false] - 禁用状态
 * @attr {boolean} [loading=false] - 加载中，显示 spinner 且阻断点击
 * @attr {('button'|'submit'|'reset')} [type='button'] - 原生 type
 * @slot - 按钮文字
 * @slot icon - 前置图标
 * @cssprop --x-button-height - 高度
 * @cssprop --x-button-radius - 圆角
 * @csspart base - 内部按钮
 * @fires x-button-click - 点击时触发，detail 为 { originalEvent }
 */
export class XButton extends HTMLElement {
  static observedAttributes = ['variant', 'size', 'disabled', 'loading', 'type'];

  #root: ShadowRoot;
  #button: HTMLButtonElement;

  constructor() {
    super();
    this.#root = this.attachShadow({ mode: 'open' });
    this.#root.innerHTML = `
      <style>
        :host { display: inline-block; }
        .base {
          height: var(--x-button-height, 2.5rem);
          border-radius: var(--x-button-radius, 0.5rem);
        }
      </style>
      <button part="base" class="base" type="button"><slot></slot></button>
    `;
    this.#button = this.#root.querySelector('button')!;
    this.#button.addEventListener('click', (originalEvent) => {
      const blocked = this.hasAttribute('disabled') || this.hasAttribute('loading');
      if (blocked) {
        originalEvent.stopImmediatePropagation();
        originalEvent.preventDefault();
        return;
      }
      this.dispatchEvent(new CustomEvent('x-button-click', {
        bubbles: true,
        composed: true,
        detail: { originalEvent },
      }));
    });
  }

  attributeChangedCallback(name: string, _old: string | null, value: string | null) {
    if (name === 'disabled') {
      this.#button.disabled = value !== null;
    }
    if (name === 'type') {
      this.#button.type = (value as 'button' | 'submit' | 'reset') ?? 'button';
    }
  }
}

customElements.define('x-button', XButton);
