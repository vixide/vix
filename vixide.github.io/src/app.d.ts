declare global {
  namespace App {
    // interface Error {}
    // interface Locals {}
    interface PageData {
      /** `<title>` text, also passed to SharePicker so a share carries the page's own title. */
      title?: string;
      /** `<meta name="description">` text, reused for the Open Graph and Twitter card description. */
      description?: string;
    }
    // interface PageState {}
    // interface Platform {}
  }
}

export {};
