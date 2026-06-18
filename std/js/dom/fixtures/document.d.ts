interface Document {
  readonly title: string;
  getElementById(elementId: string): HTMLElement | null;
}

interface HTMLElement {
  readonly id: string;
  textContent: string | null;
}
