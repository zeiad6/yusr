import React, { useEffect, useRef } from "react";
import gsap from "gsap";

interface CountUpProps {
  to: number;
  from?: number;
  duration?: number;
  decimals?: number;
  prefix?: string;
  suffix?: string;
  className?: string;
}

export const CountUp: React.FC<CountUpProps> = ({
  to,
  from = 0,
  duration = 1.2,
  decimals = 2,
  prefix = "",
  suffix = "",
  className = "",
}) => {
  const nodeRef = useRef<HTMLSpanElement>(null);
  const valRef = useRef<{ value: number }>({ value: from });

  useEffect(() => {
    valRef.current.value = from;
    const anim = gsap.to(valRef.current, {
      value: to,
      duration,
      ease: "power2.out",
      onUpdate: () => {
        if (nodeRef.current) {
          const formatted = valRef.current.value.toLocaleString(undefined, {
            minimumFractionDigits: decimals,
            maximumFractionDigits: decimals,
          });
          nodeRef.current.textContent = `${prefix}${formatted}${suffix}`;
        }
      },
    });

    return () => {
      anim.kill();
    };
  }, [to, from, duration, decimals, prefix, suffix]);

  return (
    <span ref={nodeRef} className={`tabular-nums ${className}`}>
      {prefix}
      {from.toFixed(decimals)}
      {suffix}
    </span>
  );
};
