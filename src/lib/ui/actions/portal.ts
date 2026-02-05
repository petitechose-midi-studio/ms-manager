export function portal(node: HTMLElement, target: HTMLElement | string = "body") {
  if (typeof document === "undefined") {
    return {
      destroy() {
        // noop
      },
    };
  }

  const el =
    typeof target === "string"
      ? (document.querySelector(target) as HTMLElement | null)
      : target;

  const mount = el ?? document.body;
  mount.appendChild(node);

  return {
    destroy() {
      node.parentNode?.removeChild(node);
    },
  };
}
