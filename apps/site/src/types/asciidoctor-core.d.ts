declare module "@asciidoctor/core" {
  type ConvertOptions = {
    safe?: string;
    attributes?: Record<string, unknown>;
  };

  type AsciidoctorInstance = {
    convert(input: string, options?: ConvertOptions): string;
  };

  export default function Asciidoctor(): AsciidoctorInstance;
}
