import React from "react";

interface ShinyTextProps {
  text: string;
  disabled?: boolean;
  speed?: number;
  className?: string;
}

export const ShinyText: React.FC<ShinyTextProps> = ({
  text,
  disabled = false,
  speed = 5,
  className = "",
}) => {
  const animationDuration = `${speed}s`;

  return (
    <span
      className={`inline-block bg-clip-text text-transparent ${
        disabled ? "text-[#e6c36f]" : ""
      } ${className}`}
      style={{
        backgroundImage:
          "linear-gradient(120deg, rgba(230, 195, 111, 0.7) 0%, rgba(255, 255, 255, 0.98) 50%, rgba(230, 195, 111, 0.7) 100%)",
        backgroundSize: "200% 100%",
        WebkitBackgroundClip: "text",
        animation: disabled ? "none" : `t-shimmer ${animationDuration} infinite linear`,
      }}
    >
      {text}
    </span>
  );
};
