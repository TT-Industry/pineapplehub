import matplotlib.pyplot as plt
import numpy as np
import matplotlib.patches as patches

# Setup figure with academic aesthetic (serif fonts)
plt.rcParams['font.family'] = 'serif'
fig, ax = plt.subplots(figsize=(9, 5.5))
ax.set_aspect('equal')

# Origin
plt.plot(0, 0, 'ko', markersize=6, zorder=5)
plt.text(-0.8, -0.6, 'Camera Origin\n$(0,0,0)$', fontsize=12, ha='center', va='top')

# Image Plane (at Depth Z = f)
f = 3.0
plt.plot([f, f], [-2.5, 2.5], ls='--', lw=2.5, color='#7f8c8d')
plt.text(f, 2.7, 'Image Plane\n(2D Sensor Array, $Z=f$)', fontsize=12, ha='center', color='#34495e', fontweight='bold')

# The curved surface (Cylinder cross-section)
# Z_c solving parameters
z0 = 8.5
R = 4.5
y_surf = np.linspace(-R*0.95, R*0.95, 200)
# Equation: X^2 + (Z - z0)^2 = R^2  => (using Z as the horizontal axis here)
# Z = z0 - sqrt(R^2 - Y^2)
z_surf = z0 - np.sqrt(R**2 - y_surf**2)
plt.plot(z_surf, y_surf, color='#2980b9', lw=3.0, label='Target Cylindrical Surface', zorder=2)
plt.text(z0 - R + 0.3, R*0.8, 'Target Object Surface\n$Y^2 + (Z - z_0)^2 = R^2$', fontsize=12, color='#2980b9', ha='left', fontweight='bold')

# Rays and specific mapping
# Let's shoot ray through uniformly spaced points on image plane
u_vals = np.linspace(-0.6, 0.6, 7) 
for idx, u in enumerate(u_vals):
    # u = Y / Z  => Y = u * Z
    # Substitution => (u^2 + 1)Z^2 - 2z0 Z + (z0^2 - R^2) = 0
    A = u**2 + 1
    B = -2*z0
    C = z0**2 - R**2
    disc = B**2 - 4*A*C
    
    if disc > 0:
        z_int = (-B - np.sqrt(disc)) / (2*A)
        y_int = u * z_int
        
        # Draw ray
        plt.plot([0, z_int], [0, y_int], color='#95a5a6', lw=1.2, alpha=0.8, zorder=1)
        
        # Intersect with image plane:
        y_img = u * f
        plt.plot(f, y_img, marker='s', color='#e67e22', markersize=5, zorder=4)
        
        # Intersect with surface:
        plt.plot(z_int, y_int, marker='o', color='#c0392b', markersize=6, zorder=4)
        
        # Add labels for extreme rays
        if idx == 0:
            plt.text(f-0.2, y_img, '$(p_x, p_y)$', fontsize=10, color='#d35400', ha='right', va='center')
            plt.text(z_int+0.2, y_int, '$(X, Y, Z_c)$', fontsize=10, color='#c0392b', ha='left', va='center')

# Annotations for distances
# Arc indicating radial foreshortening
plt.annotate('', xy=(z0-R, 0), xytext=(z_surf[0], y_surf[0]),
            arrowprops=dict(arrowstyle="<->", connectionstyle="arc3,rad=-0.2", color='#8e44ad', ls='--'))
plt.text(z0-R+0.1, y_surf[0]/2, 'Foreshortening\nEffect', fontsize=10, color='#8e44ad', rotation=70, va='center')

# Configure axes representing (Z, Y) space top-down view
ax.spines['top'].set_visible(False)
ax.spines['right'].set_visible(False)
ax.spines['bottom'].set_position('zero')
ax.spines['left'].set_position('zero')

# Make arrow heads
ax.plot((1), (0), ls="", marker=">", ms=8, color="k",
        transform=ax.get_yaxis_transform(), clip_on=False)
ax.plot((0), (1), ls="", marker="^", ms=8, color="k",
        transform=ax.get_xaxis_transform(), clip_on=False)
        
plt.xlabel('Z Depth Axis ($Z_c$)', loc='right', fontsize=12, style='italic')
plt.ylabel('Y Coordinate Axis', loc='top', fontsize=12, style='italic')

plt.xlim(-1.5, z0 + 1)
plt.ylim(-R*1.1, R*1.1)

plt.tight_layout()
plt.savefig('perspective_projection.svg', dpi=300, bbox_inches='tight')
