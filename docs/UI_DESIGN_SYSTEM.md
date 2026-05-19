# WinBlaze UI Design System

## Overview

WinBlaze features a modern, sophisticated design system that prioritizes clarity, performance visualization, and user delight. The interface uses contemporary design principles including glassmorphism, subtle gradients, and a carefully crafted color palette.

## Design Philosophy

### Core Principles

1. **Visual Hierarchy**: Clear distinction between primary actions, secondary information, and background elements
2. **Modern Aesthetics**: Dark theme by default with purple/blue gradient accents
3. **Performance-Focused**: Visual design that emphasizes speed and efficiency
4. **Accessibility**: High contrast ratios and clear typography for readability
5. **Consistency**: Unified design language across all components

## Color System

### Dark Theme (Primary)

#### Background Colors
- **App Background**: `#0A0E27` - Deep navy providing excellent contrast
- **Card Background**: `#151A3C` - Elevated surface color
- **Card Hover**: `#1A2047` - Interactive state
- **Subtle Background**: `#0F1329` - Input fields and secondary surfaces

#### Text Colors
- **Primary Text**: `#FFFFFF` - Full white for maximum readability
- **Secondary Text**: `#FFFFFF` @ 70% - Subdued information
- **Tertiary Text**: `#FFFFFF` @ 50% - Placeholder and hints

#### Accent Colors
- **Primary Accent**: `#6B5FFF` - Electric purple
- **Secondary Accent**: `#4B7FFF` - Bright blue
- **Success**: `#00D68F` - Vibrant green
- **Warning**: `#FFB800` - Attention yellow
- **Error**: `#FF3B3B` - Alert red

#### Gradient System
The primary gradient flows from electric purple to bright blue, creating a modern, dynamic feel:
```
Linear Gradient: #6B5FFF → #4B7FFF
```

Progress bars feature a three-color gradient indicating progression:
```
Progress Gradient: #6B5FFF → #4B7FFF → #00D68F
```

### File Type Colors
Each file type has a distinct color for quick visual identification:
- **Volumes/Drives**: `#6B5FFF` (Purple)
- **Folders**: `#4B7FFF` (Blue)
- **Regular Files**: `#00D68F` (Green)
- **Archives**: `#FFB800` (Yellow)
- **Media Files**: `#FF3B7F` (Pink)
- **Documents**: `#00B4D8` (Cyan)

## Typography

### Font Stack
- **Headers**: Segoe UI Variable Display - Modern, clean display font
- **Body Text**: Segoe UI Variable Text - Optimized for readability
- **Monospace**: Cascadia Code - For file paths and technical information

### Type Scale
- **Display**: 32px, Semi-bold
- **H1**: 24px, Semi-bold
- **H2**: 20px, Semi-bold
- **H3**: 18px, Medium
- **Body**: 14px, Regular
- **Caption**: 12px, Regular
- **Small**: 11px, Regular

## Spacing System

Based on an 8px grid for consistency:
- **XXS**: 4px
- **XS**: 8px
- **S**: 12px
- **M**: 16px
- **L**: 24px
- **XL**: 32px
- **XXL**: 48px

## Component Design

### Cards
- **Corner Radius**: 12px for modern, soft appearance
- **Border**: 1px with 20% opacity white
- **Padding**: 20px internal spacing
- **Shadow**: Subtle drop shadow for depth

### Buttons
- **Primary**: Accent color background with white text
- **Secondary**: Transparent with accent border
- **Corner Radius**: 8px
- **Padding**: 20px horizontal, 12px vertical
- **Hover State**: 10% brightness increase
- **Active State**: 10% brightness decrease

### Input Fields
- **Background**: Subtle background color
- **Border**: 1px with 10% opacity white
- **Corner Radius**: 8px
- **Padding**: 12px horizontal, 8px vertical
- **Focus State**: Accent color border

### Progress Indicators
- **Track**: Dark elevated background
- **Fill**: Animated gradient showing progression
- **Height**: 4px for subtle appearance
- **Corner Radius**: 2px

## Visual Effects

### Glassmorphism
Used for overlays and elevated surfaces:
- **Backdrop Blur**: 20px
- **Background**: 80% opacity tint
- **Border**: 1px with 20% opacity white

### Animations
- **Standard Duration**: 300ms
- **Fast Duration**: 150ms
- **Easing**: Cubic-bezier(0.4, 0, 0.2, 1)
- **Hover Transitions**: Scale and brightness
- **Page Transitions**: Fade and slide

### Shadows
Three elevation levels:
1. **Low**: 0px 2px 4px rgba(0,0,0,0.1)
2. **Medium**: 0px 4px 8px rgba(0,0,0,0.15)
3. **High**: 0px 8px 16px rgba(0,0,0,0.2)

## Treemap Visualization

The treemap uses the accent color system to create visually striking data representations:
- **Gradient Overlays**: Subtle gradients indicate file types
- **Hover Effects**: Brightening and elevation
- **Selection**: Accent color border with glow effect
- **Labels**: High contrast with background-aware coloring

## Responsive Design

### Breakpoints
- **Compact**: < 640px
- **Medium**: 641px - 1007px
- **Expanded**: ≥ 1008px

### Adaptive Layouts
- **Navigation**: Collapses to hamburger menu on compact
- **Cards**: Stack vertically on compact screens
- **Typography**: Scales down appropriately for mobile

## Accessibility

### Color Contrast
All text meets WCAG AA standards:
- **Normal Text**: Minimum 4.5:1 contrast ratio
- **Large Text**: Minimum 3:1 contrast ratio
- **Interactive Elements**: Minimum 3:1 contrast ratio

### Focus Indicators
- **Keyboard Navigation**: Clear focus rings using accent color
- **Tab Order**: Logical flow through interface
- **Screen Reader**: Semantic HTML and ARIA labels

## Implementation Guidelines

### Using the Design System

1. **Always use design tokens**: Reference color and spacing variables rather than hard-coding values
2. **Maintain consistency**: Use established patterns and components
3. **Test in both themes**: Ensure components work in dark and light modes
4. **Consider performance**: Minimize complex gradients and shadows on frequently updated elements
5. **Respect the grid**: Align elements to the 8px spacing system

### Example Usage

```xaml
<!-- Modern Card Component -->
<Border Style="{StaticResource WinBlazeCardStyle}">
    <StackPanel Spacing="16">
        <TextBlock Text="Disk Usage" 
                   FontFamily="{StaticResource WinBlazeHeaderFont}"
                   FontSize="20"
                   Foreground="{StaticResource WinBlazeTextPrimaryBrush}"/>
        <ProgressBar Value="75" 
                     Foreground="{StaticResource WinBlazeProgressFillBrush}"
                     Background="{StaticResource WinBlazeProgressTrackBrush}"/>
    </StackPanel>
</Border>

<!-- Modern Button -->
<Button Content="Start Scan" 
        Style="{StaticResource WinBlazeModernButtonStyle}"/>

<!-- Gradient Accent -->
<Border Background="{StaticResource WinBlazeAccentGradientBrush}"
        CornerRadius="8"
        Padding="16">
    <TextBlock Text="1.8x Faster than competitors"
               Foreground="{StaticResource WinBlazeTextOnAccentBrush}"/>
</Border>
```

## Future Enhancements

- **Light Theme**: Full light mode with automatic OS theme detection
- **Custom Themes**: User-definable color schemes
- **Animations**: More sophisticated transitions and micro-interactions
- **Data Visualizations**: Enhanced charts and graphs with the design system
- **Icons**: Custom icon set matching the design language

## Design Tools

For designers working on WinBlaze:
- **Figma**: Component library available at [link]
- **Color Palette**: Exportable as ASE/JSON
- **Icon Set**: SVG format with multiple weights
- **Typography**: Variable font files included