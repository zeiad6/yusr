import { useEffect, useRef } from "react";
import { fitPaper } from "../utils/paper";

/** Live A4 preview of a document built with `paperHtml`. */
export default function PaperFrame({ html, title }: { html: string; title: string }) {
  const ref = useRef<HTMLIFrameElement>(null);
  useEffect(() => {
    const onResize = () => ref.current && fitPaper(ref.current);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return <iframe ref={ref} className="paper-frame" title={title} srcDoc={html} onLoad={(e) => fitPaper(e.currentTarget)} />;
}
